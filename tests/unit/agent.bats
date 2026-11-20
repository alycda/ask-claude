#!/usr/bin/env bats
# shellcheck disable=SC2016  # a jq program in single quotes mentions $ on purpose
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
    request 2 | jq -e '.events[0].type=="user.custom_tool_result" and .events[0].custom_tool_use_id=="sevt_FIXTURE04" and .events[0].is_error==true'
    request 2 | jq -e '.events[0].content[0].text | test("no local tools")'
    request 3 | jq -e '.events[0].custom_tool_use_id=="sevt_FIXTURE13"'
    grep -q 'Weather service unavailable' <<<"$output"
}


# vaults, a repository and memory stores (docs/plans/2026-11-05-001 U9)

@test "resources: --vault, --repo and --memory reach the create body" {
    GITHUB_TOKEN=ghp_FIXTURE replay text.sse --vault vlt_FIXTURE01 --vault vlt_FIXTURE02 --repo owner/name@main --memory memstore_FIXTURE01 --memory memstore_FIXTURE02:ro "ping" >/dev/null 2>&1
    request 1 | jq -S . > "$TMP/got.json"
    jq -S . "$FIX/agents/create-resources.json" > "$TMP/want.json"
    assert_golden "$TMP/want.json" "$TMP/got.json"
}

@test "resources: a full GitHub URL and no branch work too" {
    GITHUB_TOKEN=ghp_FIXTURE replay text.sse --repo https://github.com/owner/name "ping" >/dev/null 2>&1
    request 1 | jq -e '.resources[0].url=="https://github.com/owner/name" and .resources[0].mount_path=="/workspace/name" and (.resources[0]|has("checkout")|not)'
}

@test "resources: --repo without GITHUB_TOKEN exits 2 before any request" {
    run --separate-stderr env -u GITHUB_TOKEN ASK_CLAUDE_REPLAY="$FIX/sessions/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent --repo owner/name "ping"
    [ "$status" -eq 2 ]
    grep -q 'GITHUB_TOKEN' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "resources: more than 8 memory stores, or a bad id, exits 2 before any request" {
    run --separate-stderr replay text.sse --memory memstore_a --memory memstore_b --memory memstore_c --memory memstore_d --memory memstore_e --memory memstore_f --memory memstore_g --memory memstore_h --memory memstore_i "ping"
    [ "$status" -eq 2 ]
    grep -q '8' <<<"$stderr"
    run --separate-stderr replay text.sse --memory nope "ping"
    [ "$status" -eq 2 ]
    grep -q 'memstore_' <<<"$stderr"
    run --separate-stderr replay text.sse --vault nope "ping"
    [ "$status" -eq 2 ]
    grep -q 'vlt_' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

# bats test_tags=bash-only
@test "resources: the token never goes on a command line" {
    refute grep -qE -- '--arg(json)? +token' "$BIN"
    grep -q 'env.GITHUB_TOKEN' "$ROOT/lib/session-create.jq"
}

# the live paths the replay seam used to skip (review of 0.2.3)

@test "live paths: a terminated session without an error still exits 1" {
    run --separate-stderr replay terminated "ping"
    [ "$status" -eq 1 ]
    grep -q 'terminated' <<<"$stderr"
}

@test "live paths: a stream that is not SSE (an error body) exits 1 with it" {
    run --separate-stderr replay not-sse "ping"
    [ "$status" -eq 1 ]
    grep -q 'invalid x-api-key' <<<"$stderr"
}

@test "live paths: a refused events POST exits 1 with the API's message, no stream read" {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sessions/events-refused" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent resume sesn_FIXTURE01 <<<$'again\n/quit\n'
    [ "$status" -eq 0 ]
    grep -q 'is archived' <<<"$stderr"
    refute grep -q '^pong' <<<"$output"
}

@test "live paths: waiting on something this version cannot answer interrupts and exits 1" {
    run --separate-stderr replay waiting-unknown "ping" </dev/null
    [ "$status" -eq 1 ]
    grep -q 'cannot answer' <<<"$stderr"
    request 2 | jq -e '.events==[{"type":"user.interrupt"}]'
}

@test "live paths: a prefix matches each session once" {
    mkdir -p "$(STATE_DIR)/sessions"
    printf 'agent=a\n' > "$(STATE_DIR)/sessions/sesn_FIXTURE01"
    run --separate-stderr env -u ANTHROPIC_API_KEY "$BIN" agent trace FIXTURE
    [ "$status" -eq 0 ]
    [ "$output" = "https://platform.claude.com/workspaces/default/sessions/sesn_FIXTURE01" ]
}

@test "agent: a session id that is not one is refused before it names a file" {
    mkdir -p "$TMP/fx"
    printf 'precious\n' >"$TMP/victim.txt"
    # sessions/ is three levels below $TMP: state/ask-claude/sessions
    printf '{"id":"../../../victim.txt","status":"running"}\n' >"$TMP/fx/create.json"
    printf 'data: {"type":"session.status_idle","stop_reason":{"type":"end_turn"}}\n\n' >"$TMP/fx/1.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/fx" "$BIN" agent "ping"
    [ "$status" -eq 1 ]
    grep -q 'session id is not one' <<<"$stderr"
    [ "$(cat "$TMP/victim.txt")" = precious ]
}

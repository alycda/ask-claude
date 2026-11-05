#!/usr/bin/env bats
load helpers

# A session that stops with requires_action on a tool call: asked at a
# terminal, denied otherwise. ASK_CLAUDE_CONFIRM=allow|deny|ask overrides
# the default (ask on a terminal, deny off it); bats has no terminal, so the
# prompt itself is checked by hand under `script`.

export ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE
replay() { ASK_CLAUDE_REPLAY="$FIX/sessions/$1" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent "${@:2}"; }
request() { sed -n "$1p" "$TMP/requests.jsonl"; }

@test "confirm: off a terminal the default is deny, and stderr says how to change it" {
    run --separate-stderr replay confirm "ping" </dev/null
    [ "$status" -eq 0 ]
    request 2 | jq -e '.events[0].result=="deny"'
    grep -q 'denied bash' <<<"$stderr"
    grep -q 'echo pong' <<<"$stderr"
    grep -q 'ASK_CLAUDE_CONFIRM' <<<"$stderr"
}

@test "confirm: ASK_CLAUDE_CONFIRM=allow answers allow and says so" {
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay confirm "ping" </dev/null
    [ "$status" -eq 0 ]
    request 2 | jq -e '.events==[{"type":"user.tool_confirmation","tool_use_id":"sevt_FIXTURE04","result":"allow"}]'
    grep -q 'allowed bash' <<<"$stderr"
    [ "$output" = "$(printf '(bash) \npong')" ]
}

@test "confirm: ASK_CLAUDE_CONFIRM=deny is the explicit form of the default" {
    ASK_CLAUDE_CONFIRM=deny run --separate-stderr replay confirm "ping" </dev/null
    [ "$status" -eq 0 ]
    request 2 | jq -e '.events[0].result=="deny"'
}

@test "confirm: ASK_CLAUDE_CONFIRM=ask off a terminal falls back to deny" {
    ASK_CLAUDE_CONFIRM=ask run --separate-stderr replay confirm "ping" </dev/null
    [ "$status" -eq 0 ]
    request 2 | jq -e '.events[0].result=="deny"'
    grep -q 'not a terminal' <<<"$stderr"
}

@test "confirm: an unknown ASK_CLAUDE_CONFIRM exits 2 before any request" {
    ASK_CLAUDE_CONFIRM=yes run --separate-stderr replay confirm "ping" </dev/null
    [ "$status" -eq 2 ]
    grep -q 'ASK_CLAUDE_CONFIRM' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "confirm: a custom tool call is answered the same way whatever the policy" {
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay custom-tool "ping" </dev/null
    [ "$status" -eq 0 ]
    request 2 | jq -e '.events[0].type=="user.custom_tool_result" and .events[0].is_error==true'
}

# confirm_fixture <dir>: turn 1 asks for a bash tool call, turn 2 ends. Each
# stream carries a session.usage, as live streams do (the fold needs one).
confirm_fixture() {
    local usage='data: {"type":"session.usage","usage":{"input_tokens":1,"output_tokens":1}}'
    mkdir -p "$1"
    printf '%s\n\n' 'data: {"type":"agent.tool_use","id":"sevt_T1","name":"bash","input":{"command":"echo pong"}}' \
        "$usage" 'data: {"type":"session.status_idle","stop_reason":{"type":"requires_action","event_ids":["sevt_T1"]}}' >"$1/1.sse"
    printf '%s\n\n' "$usage" 'data: {"type":"session.status_idle","stop_reason":{"type":"end_turn"}}' >"$1/2.sse"
}

@test "confirm: an always answer left by an earlier process with this PID answers nothing" {
    confirm_fixture "$TMP/fx"
    mkdir -p "$XDG_STATE_HOME/ask-claude"
    # plant the grant for the PID the script will run as: exec keeps the PID,
    # and the script is run by bash directly with the usage_* variables set
    # shellcheck disable=SC2016  # $$ is the inner shell's PID
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/fx" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        usage_cmd=agent usage_prompt=ping usage_model=m usage_max_tokens=16000 usage_budget=100 \
        usage_agent=agent_FIXTURE usage_environment=env_FIXTURE \
        bash -c ': >"$XDG_STATE_HOME/ask-claude/.always.$$"; exec bash "$1"' _ "$BIN" </dev/null
    [ "$status" -eq 0 ]
    request 2 | jq -e '.events[0].result=="deny"'
    [ -z "$(find "$XDG_STATE_HOME/ask-claude" -maxdepth 1 -name '.always.*')" ]
}

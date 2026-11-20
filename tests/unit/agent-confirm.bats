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

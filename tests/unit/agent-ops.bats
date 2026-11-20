#!/usr/bin/env bats
# shellcheck disable=SC2016  # a jq program in single quotes mentions $ on purpose
load helpers

# `agent budget <sesn> <cents>`, `agent archive <sesn>`, `agent trace <sesn>`.
# Replay answers POST /v1/sessions/{id} with the body it was sent and the
# archive with a terminated session, unless the fixture directory holds an
# update.json or archive.json (ops-running/ has the API's refusal).

export ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE
ops() { ASK_CLAUDE_REPLAY="$FIX/sessions/$1" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent "${@:2}"; }
request() { sed -n "$1p" "$TMP/requests.jsonl"; }
STATE_DIR() { echo "$XDG_STATE_HOME/ask-claude"; }

@test "budget: sets a new cap in cents and prints it" {
    run --separate-stderr ops ops budget sesn_FIXTURE01 250
    [ "$status" -eq 0 ]
    request 1 | jq -e '.budget=={"type":"limit","max_list_cost":{"amount":"250","currency":"USD"}}'
    grep -q '\$2\.50' <<<"$output"
}

@test "budget: 0 removes the cap, for good" {
    run --separate-stderr ops ops budget sesn_FIXTURE01 0
    [ "$status" -eq 0 ]
    request 1 | jq -e '.budget==null'
    grep -qi 'none' <<<"$output"
}

@test "budget: anything but whole cents exits 2 before any request" {
    for c in abc 1.5 ""; do # a leading dash is a flag to usage, refused before the script runs
        run --separate-stderr ops ops budget sesn_FIXTURE01 "$c"
        [ "$status" -eq 2 ]
    done
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "archive: an idle session is archived and forgotten here" {
    mkdir -p "$(STATE_DIR)/sessions"
    printf 'agent=agent_FIXTURE\n' > "$(STATE_DIR)/sessions/sesn_FIXTURE01"
    run --separate-stderr ops ops archive FIXTURE01
    [ "$status" -eq 0 ]
    grep -q 'archived sesn_FIXTURE01' <<<"$output"
    [ ! -e "$(STATE_DIR)/sessions/sesn_FIXTURE01" ]
}

@test "archive: a running session is refused by the API, exit 1 with its message" {
    run --separate-stderr ops ops-running archive sesn_FIXTURE01
    [ "$status" -eq 1 ]
    grep -q 'cannot be archived while its status is "running"' <<<"$stderr"
}

@test "trace: prints the Console URL, no key needed" {
    run --separate-stderr env -u ANTHROPIC_API_KEY "$BIN" agent trace sesn_FIXTURE01
    [ "$status" -eq 0 ]
    [ "$output" = "https://platform.claude.com/workspaces/default/sessions/sesn_FIXTURE01" ]
    ASK_CLAUDE_WORKSPACE=wrkspc_x run --separate-stderr env -u ANTHROPIC_API_KEY "$BIN" agent trace sesn_FIXTURE01
    [ "$output" = "https://platform.claude.com/workspaces/wrkspc_x/sessions/sesn_FIXTURE01" ]
}

@test "ops: an id that is neither a session id nor a prefix of one started here exits 2" {
    run --separate-stderr ops ops budget nope 5
    [ "$status" -eq 2 ]
    run --separate-stderr env -u ANTHROPIC_API_KEY "$BIN" agent trace nope
    [ "$status" -eq 2 ]
}

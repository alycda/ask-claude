#!/usr/bin/env bats
load helpers

# What is refused with exit 2 before any key check, request or history
# file. tests/unit/provider.bats pins the provider flags.

refused() { # args...
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "$@"
    [ "$status" -eq 2 ]
    [ ! -e "$TMP/requests.jsonl" ]
    [ -z "$(ls "$XDG_STATE_HOME"/ask-claude/*.json 2>/dev/null)" ]
}

# sent() { # want args...: exit 0, and the one turn sent is want
#     local want=$1; shift
#     rm -f "$TMP/requests.jsonl"
#     run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "$@"
#     [ "$status" -eq 0 ]
#     [ "$(jq -r '.messages[0].content' "$TMP/requests.jsonl")" = "$want" ]
# }

@test "refuse: -p anthropic is the default and still works" {
    unset ANTHROPIC_API_KEY
    run --separate-stderr env -u ANTHROPIC_API_KEY ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" -p anthropic "hi"
    [ "$status" -eq 2 ]
    refute grep -q 'not implemented' <<<"$stderr"
    grep -q 'ANTHROPIC_API_KEY' <<<"$stderr"
}

@test "refuse: --base-url over plain http, except on localhost" {
    refused --base-url http://api.example.com "hi"
    grep -q 'must be https' <<<"$stderr"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" --base-url http://localhost:8080 "hi"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
}

@test "refuse: --base-url that only looks like localhost (user info before the real host)" {
    local u
    for u in http://localhost:@example.com http://127.0.0.1:1@example.com \
        'http://[::1]:x@example.com' http://localhost:8080@example.com/ \
        http://localhost.example.com http://127.0.0.1.example.com https://u:p@example.com; do
        refused --base-url "$u" "hi"
    done
    for u in http://localhost http://127.0.0.1:9/x 'http://[::1]:1/' https://api.anthropic.com; do
        run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" --base-url "$u" "hi"
        [ "$status" -eq 0 ]
    done
}

# disable_help_subcommand makes `help` the positional prompt, so a bare
# `help`, typed from habit (`git help`, `cargo help`), was sent as a turn.
# The prompt is refused when it is the word alone; after `--` it is sent.
@test "refuse: a bare help points at --help instead of sending it" {
    local a
    for a in help HELP Help "-m claude-opus-5-5 help" "-e low help"; do
        # shellcheck disable=SC2086  # each case is several words on purpose
        refused $a
        grep -q -- '--help' <<<"$stderr"
        grep -q -- '-- help' <<<"$stderr"
    done
}

# @test "refuse: agent help, the same, before a session is created" {
#     ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE refused agent help
#     grep -q -- '--help' <<<"$stderr"
#     grep -q -- 'agent -- help' <<<"$stderr"
# }

# @test "refuse: a bare help is refused before the key check" {
#     run --separate-stderr env -u ANTHROPIC_API_KEY ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" help
#     [ "$status" -eq 2 ]
#     [ ! -e "$TMP/requests.jsonl" ]
#     grep -q -- '--help' <<<"$stderr"
#     refute grep -q 'ANTHROPIC_API_KEY' <<<"$stderr"
# }

# @test "refuse: help after --, or as the start of a longer prompt, is sent" {
#     sent help -- help
#     sent help -m claude-opus-5-5 -- help
#     sent "help me with this" "help me with this"
#     run --separate-stderr env ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE \
#         ASK_CLAUDE_REPLAY="$FIX/sessions/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent -- help
#     [ "$status" -eq 0 ]
#     [ "$output" = "pong" ]
#     grep -q '"text":"help"' "$TMP/requests.jsonl"
# }

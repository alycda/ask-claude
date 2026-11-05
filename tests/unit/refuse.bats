#!/usr/bin/env bats
load helpers

# What the bash version will never build is refused with exit 2 before any
# key check, request or history file: the OpenAI shape is the Rust port's
# (docs/plan.md, phase 3). tests/unit/provider.bats pins the flags.

refused() { # args...
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "$@"
    [ "$status" -eq 2 ]
    [ ! -e "$TMP/requests.jsonl" ]
    [ -z "$(ls "$XDG_STATE_HOME"/ask-claude/*.json 2>/dev/null)" ]
}

@test "refuse: -p openai, with the pointer to the plan" {
    refused -p openai "hi"
    grep -q '^ask-claude: --provider openai is not implemented in the bash version, see docs/plan.md$' <<<"$stderr"
}

@test "refuse: -p anthropic is the default and still works" {
    unset ANTHROPIC_API_KEY
    run --separate-stderr env -u ANTHROPIC_API_KEY ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" -p anthropic "hi"
    [ "$status" -eq 2 ]
    ! grep -q 'not implemented' <<<"$stderr"
    grep -q 'ANTHROPIC_API_KEY' <<<"$stderr"
}

@test "refuse: --base-url over plain http, except on localhost" {
    refused --base-url http://api.example.com "hi"
    grep -q 'must be https' <<<"$stderr"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" --base-url http://localhost:8080 "hi"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
}

# agent (docs/plans/2026-11-05-001): declared in the spec, refused until each
# piece is built; the feat commit that builds one deletes its line here.

@test "refuse: agent and its subcommands, with the version and the plan" {
    for args in "agent files sesn_x" "agent budget sesn_x 5" "agent archive sesn_x" "agent trace sesn_x"; do
        # shellcheck disable=SC2086
        refused $args
        grep -qE "^ask-claude: ${args%% *}.* is not implemented in ask-claude [0-9.]+, see docs/plans/2026-11-05-001" <<<"$stderr"
    done
}

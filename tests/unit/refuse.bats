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

# bats test_tags=bash-only
@test "refuse: -p openai, with the pointer to the plan" {
    refused -p openai "hi"
    grep -q '^ask-claude: --provider openai is not implemented in the bash version, see docs/plan.md$' <<<"$stderr"
}

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



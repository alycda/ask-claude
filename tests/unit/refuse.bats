#!/usr/bin/env bats
load helpers

# What the spec declares but 0.1.0 does not build is refused with exit 2 and
# "not implemented in ask-claude <version>, see docs/plan.md" on stderr,
# before any key check, request or history file. Each feature commit that
# implements one of these deletes its test here.

refused() { # args...
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "$@"
    [ "$status" -eq 2 ]
    grep -qE 'not implemented in ask-claude [0-9]+\.[0-9]+\.[0-9]+, see docs/plan\.md$' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
    [ -z "$(ls "$XDG_STATE_HOME"/ask-claude/*.json 2>/dev/null)" ]
}

@test "refuse: -p/--provider other than anthropic" {
    refused -p openai "hi"
    grep -q '^ask-claude: --provider openai is not implemented' <<<"$stderr"
}

@test "refuse: --base-url" {
    refused --base-url https://api.venice.ai/api/v1 "hi"
    grep -q '^ask-claude: --base-url is not implemented' <<<"$stderr"
}

@test "refuse: -p anthropic is the default and still works" {
    unset ANTHROPIC_API_KEY
    run --separate-stderr env -u ANTHROPIC_API_KEY ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" -p anthropic "hi"
    [ "$status" -eq 2 ]
    ! grep -q 'not implemented' <<<"$stderr"
    grep -q 'ANTHROPIC_API_KEY' <<<"$stderr"
}

@test "refuse: the version in the message is the script's own" {
    refused --base-url https://api.venice.ai/api/v1 "hi"
    v="$(sed -n 's/^readonly ASK_CLAUDE_VERSION=//p' "$BIN")"
    [ -n "$v" ]
    grep -q "not implemented in ask-claude $v," <<<"$stderr"
}

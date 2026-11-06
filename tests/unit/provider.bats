#!/usr/bin/env bats
load helpers

@test "provider: -p --provider binds and defaults to anthropic" {
    run usage explain -f "$BIN" -- ask-claude -p openai "hi"
    grep -qE '^\s+flag\s+--provider\s+openai' <<<"$output"
    run usage explain -f "$BIN" -- ask-claude "hi"
    grep -qE '^\s+flag\s+--provider\s+anthropic' <<<"$output"
}

# bats test_tags=bash-only
@test "provider: openai is refused by the bash version, exit 2" {
    run --separate-stderr "$BIN" -p openai "hi"
    [ "$status" -eq 2 ]
    grep -q 'not implemented' <<<"$stderr"
}

@test "provider: --base-url binds; Venice is openai at its own base URL" {
    run usage explain -f "$BIN" -- ask-claude -p openai --base-url https://api.venice.ai/api/v1 "hi"
    grep -qE '^\s+flag\s+--provider\s+openai' <<<"$output"
    grep -qE '^\s+flag\s+--base-url\s+https://api.venice.ai/api/v1' <<<"$output"
    ! grep -q refused <<<"$output"
}

@test "provider: an unknown provider exits 2" {
    run --separate-stderr "$BIN" -p nope "hi"
    [ "$status" -eq 2 ]
}

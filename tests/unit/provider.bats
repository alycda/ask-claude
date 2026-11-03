#!/usr/bin/env bats
load helpers

@test "provider: -p --provider binds and defaults to anthropic" {
    run usage explain -f "$BIN" -- ask-claude -p openai "hi"
    grep -qE '^\s+flag\s+--provider\s+openai' <<<"$output"
    run usage explain -f "$BIN" -- ask-claude "hi"
    grep -qE '^\s+flag\s+--provider\s+anthropic' <<<"$output"
}

@test "provider: openai and venice are refused by the bash version, exit 2" {
    for p in openai venice; do
        run --separate-stderr "$BIN" -p $p "hi"
        [ "$status" -eq 2 ]
        grep -q 'not implemented' <<<"$stderr"
    done
}

@test "provider: an unknown provider exits 2" {
    run --separate-stderr "$BIN" -p nope "hi"
    [ "$status" -eq 2 ]
}

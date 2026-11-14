#!/usr/bin/env bats
load helpers

@test "provider: -p --provider binds and defaults to anthropic" {
    run usage explain -f "$(spec_file)" -- ask-claude -p openai "hi"
    grep -qE '^\s+flag\s+--provider\s+openai' <<<"$output"
    run usage explain -f "$(spec_file)" -- ask-claude "hi"
    grep -qE '^\s+flag\s+--provider\s+anthropic' <<<"$output"
}

# bats test_tags=bash-only
@test "provider: openai is refused by the bash version, exit 2" {
    run --separate-stderr "$BIN" -p openai "hi"
    [ "$status" -eq 2 ]
    grep -q 'not implemented' <<<"$stderr"
}

@test "provider: --base-url binds; Venice is openai at its own base URL" {
    run usage explain -f "$(spec_file)" -- ask-claude -p openai --base-url https://api.venice.ai/api/v1 "hi"
    grep -qE '^\s+flag\s+--provider\s+openai' <<<"$output"
    grep -qE '^\s+flag\s+--base-url\s+https://api.venice.ai/api/v1' <<<"$output"
    refute grep -q refused <<<"$output"
}

@test "provider: an unknown provider exits 2" {
    run --separate-stderr "$BIN" -p nope "hi"
    [ "$status" -eq 2 ]
}

# ANTHROPIC_BASE_URL: the official SDK's name, the environment twin of
# --base-url for the Anthropic provider, under the same gate. The flag wins
# over it. A value from the environment that is not the default root is
# announced on stderr in one line, since the variable is often exported for
# other tools and the key goes where it points. That the request then goes
# there is rust/tests/http.rs (a localhost mock, on both binaries) and
# events.bats (the event log's host).

@test "base url: the spec names ANTHROPIC_BASE_URL as --base-url's env, and the flag wins" {
    run env ANTHROPIC_BASE_URL=http://localhost:9 usage explain -f "$(spec_file)" -- ask-claude "hi"
    grep -qE '^\s+flag\s+--base-url\s+http://localhost:9\s+env ANTHROPIC_BASE_URL$' <<<"$output"
    run env ANTHROPIC_BASE_URL=http://localhost:9 usage explain -f "$(spec_file)" -- ask-claude --base-url http://localhost:8080 "hi"
    grep -qE '^\s+flag\s+--base-url\s+http://localhost:8080\s+argv' <<<"$output"
}

@test "base url: ANTHROPIC_BASE_URL passes the same gate as --base-url, exit 2 before any request" {
    run --separate-stderr env ANTHROPIC_BASE_URL=http://example.com ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "hi"
    [ "$status" -eq 2 ]
    # the message names where the value came from
    [ "$stderr" = "ask-claude: ANTHROPIC_BASE_URL must be https:// (or http:// on localhost), got: http://example.com" ]
    [ ! -e "$TMP/requests.jsonl" ]
    run --separate-stderr env ANTHROPIC_BASE_URL=http://localhost:@example.com ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" "hi"
    [ "$status" -eq 2 ]
    grep -q 'ANTHROPIC_BASE_URL must not carry user info' <<<"$stderr"
    # even a subcommand that sends nothing: the gate runs before dispatch
    run --separate-stderr env ANTHROPIC_BASE_URL=http://example.com "$BIN" list
    [ "$status" -eq 2 ]
    grep -q 'must be https' <<<"$stderr"
}

@test "base url: from the environment it is announced on stderr once; the flag wins and says nothing" {
    run --separate-stderr env ANTHROPIC_BASE_URL=http://localhost:9/some/path ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" "hi"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
    [ "$(grep -c 'ANTHROPIC_BASE_URL' <<<"$stderr")" -eq 1 ]
    grep -qx 'ask-claude: ANTHROPIC_BASE_URL is set; requests go to localhost:9' <<<"$stderr"
    # the flag wins, over a value the gate would refuse too
    run --separate-stderr env ANTHROPIC_BASE_URL=http://example.com ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" --base-url http://localhost:8080 "hi"
    [ "$status" -eq 0 ]
    refute grep -q 'ANTHROPIC_BASE_URL' <<<"$stderr"
    # the default root is not announced, with or without its trailing slash
    run --separate-stderr env ANTHROPIC_BASE_URL=https://api.anthropic.com/ ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" "hi"
    [ "$status" -eq 0 ]
    refute grep -q 'ANTHROPIC_BASE_URL' <<<"$stderr"
    # an empty variable is no variable
    run --separate-stderr env ANTHROPIC_BASE_URL= ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" "hi"
    [ "$status" -eq 0 ]
    refute grep -q 'ANTHROPIC_BASE_URL' <<<"$stderr"
}

# bats test_tags=bash-only
@test "base url: the recording script reads ANTHROPIC_BASE_URL, and the test-only name is gone" {
    # the retired name, spelled so this file does not match it
    local old="ASK_CLAUDE_""BASE_URL"
    grep -q 'ANTHROPIC_BASE_URL' "$ROOT/tests/record-session.sh"
    refute grep -rqF "$old" "$ROOT/ask-claude" "$ROOT/lib" "$ROOT/tests" "$ROOT/README.md" "$ROOT/justfile" "$ROOT/.env.example" \
        "$ROOT/rust/src" "$ROOT/rust/tests"
}

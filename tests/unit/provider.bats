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

@test "base url: what the notice and the refusals print of the root goes through safe" {
    # the variable is not typed at this prompt: an https root with a title
    # sequence passes the gate, and its host is announced without it
    run --separate-stderr env ANTHROPIC_BASE_URL=$'https://x\e]0;pwned\a/' ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" "hi"
    [ "$status" -eq 0 ]
    grep -qxF 'ask-claude: ANTHROPIC_BASE_URL is set; requests go to x]0;pwned' <<<"$stderr"
    refute grep -q $'[\e\a]' <<<"$stderr"
    run --separate-stderr env ANTHROPIC_BASE_URL=$'http://x\e[2J/' "$BIN" "hi"
    [ "$status" -eq 2 ]
    [ "$stderr" = "ask-claude: ANTHROPIC_BASE_URL must be https:// (or http:// on localhost), got: http://x[2J/" ]
    run --separate-stderr env ANTHROPIC_BASE_URL=$'http://localhost:@x\e[2J' "$BIN" "hi"
    [ "$status" -eq 2 ]
    [ "$stderr" = "ask-claude: ANTHROPIC_BASE_URL must not carry user info (an @), got: http://localhost:@x[2J" ]
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

# bats test_tags=rust-only
@test "base url: with -p openai the Anthropic variable is ignored and OPENAI_BASE_URL applies" {
    run --separate-stderr env ANTHROPIC_BASE_URL=http://example.com OPENAI_BASE_URL=http://localhost:9 OPENAI_API_KEY=sk-openai-test-not-a-key ASK_CLAUDE_REPLAY="$FIX/openai/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" -p openai -m gpt-4.1-mini "hi"
    [ "$status" -ne 2 ]
    [ "$(grep -c 'must be https' <<<"$stderr")" -eq 0 ]
    [ "$(grep -c 'ANTHROPIC_BASE_URL' <<<"$stderr")" -eq 0 ]
    # the request went to OPENAI_BASE_URL's host, as the event log records it
    jq -e '.host=="localhost:9"' "$XDG_STATE_HOME/ask-claude/events.jsonl"
}

# -p openai, the chat-completions shape (R7): the Rust port's alone, so
# every scenario is rust-only. The streams under fixtures/openai/ are
# hand-written from the shape tests/providers/openai-compatible.hurl
# asserts; rust/tests/http.rs holds the headers and the path.

openai() { # args...: the one shot through the replay seam, the request logged
    run --separate-stderr env OPENAI_API_KEY=sk-openai-test-not-a-key ASK_CLAUDE_REPLAY="$FIX/openai/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" -p openai "$@"
}

# bats test_tags=rust-only
@test "openai: OPENAI_BASE_URL passes the same gate as --base-url, exit 2 before any request" {
    openai -m gpt-4.1-mini "hi"
    [ "$status" -eq 0 ]
    run --separate-stderr env OPENAI_BASE_URL=http://example.com OPENAI_API_KEY=k ASK_CLAUDE_REPLAY="$FIX/openai/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/refused.jsonl" "$BIN" -p openai "hi"
    [ "$status" -eq 2 ]
    [ "$stderr" = "ask-claude: OPENAI_BASE_URL must be https:// (or http:// on localhost), got: http://example.com" ]
    [ ! -e "$TMP/refused.jsonl" ]
    run --separate-stderr env OPENAI_BASE_URL=http://localhost:@example.com OPENAI_API_KEY=k ASK_CLAUDE_REPLAY="$FIX/openai/text.sse" "$BIN" -p openai "hi"
    [ "$status" -eq 2 ]
    grep -q 'OPENAI_BASE_URL must not carry user info' <<<"$stderr"
    # the flag is gated under its own name
    run --separate-stderr env OPENAI_API_KEY=k ASK_CLAUDE_REPLAY="$FIX/openai/text.sse" "$BIN" -p openai --base-url http://example.com "hi"
    [ "$status" -eq 2 ]
    grep -q '^ask-claude: --base-url must be https' <<<"$stderr"
}

# bats test_tags=rust-only
@test "openai: the one shot prints the reply and logs the chat-completions body" {
    openai -m gpt-4.1-mini -S "be brief" --max-tokens 64 "hi"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
    jq -e '. == {model: "gpt-4.1-mini", stream: true, max_completion_tokens: 64, stream_options: {include_usage: true},
        messages: [{role: "system", content: "be brief"}, {role: "user", content: "hi"}]}' "$TMP/requests.jsonl"
}

# bats test_tags=rust-only
@test "openai: the model is OPENAI_MODEL's when -m is not given, and -m wins" {
    OPENAI_MODEL=gemini-4-argon openai "hi"
    [ "$status" -eq 0 ]
    OPENAI_MODEL=gemini-4-argon openai -m gpt-4.1-mini "hi"
    [ "$status" -eq 0 ]
    [ "$(jq -r .model "$TMP/requests.jsonl")" = "gemini-4-argon
gpt-4.1-mini" ]
}

# bats test_tags=rust-only
@test "openai: a chat sends the earlier reply back as text, and saves it as the Messages path does" {
    run --separate-stderr env OPENAI_API_KEY=k ASK_CLAUDE_REPLAY="$FIX/openai/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" -p openai -m gpt-4.1-mini <<<$'first\nsecond\n/quit'
    [ "$status" -eq 0 ]
    jq -s -e '.[1].messages == [{role: "user", content: "first"}, {role: "assistant", content: "pong"}, {role: "user", content: "second"}]' "$TMP/requests.jsonl"
    jq -e '.[1] == {role: "assistant", content: [{type: "text", text: "pong"}]} and length == 4' "$XDG_STATE_HOME"/ask-claude/*.json
}

# bats test_tags=rust-only
@test "openai: the Messages API's server tools are refused, exit 2 before any request" {
    for flags in "--web" "--mcp https://mcp.example.com" "-s pptx" "--advisor claude-opus-5-5"; do
        # shellcheck disable=SC2086
        openai -m gpt-4.1-mini $flags "hi"
        [ "$status" -eq 2 ]
        grep -q "^ask-claude: ${flags%% *} works with -p anthropic only$" <<<"$stderr"
        [ ! -e "$TMP/requests.jsonl" ]
    done
}

# bats test_tags=bash-only
@test "base url: the recording script reads ANTHROPIC_BASE_URL, and the test-only name is gone" {
    # the retired name, spelled so this file does not match it
    local old="ASK_CLAUDE_""BASE_URL"
    grep -q 'ANTHROPIC_BASE_URL' "$ROOT/tests/record-session.sh"
    refute grep -rqF "$old" "$ROOT/ask-claude" "$ROOT/lib" "$ROOT/tests" "$ROOT/README.md" "$ROOT/justfile" "$ROOT/.env.example" \
        "$ROOT/rust/src" "$ROOT/rust/tests"
}

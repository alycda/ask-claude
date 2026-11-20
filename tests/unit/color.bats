#!/usr/bin/env bats
load helpers

# --color on stdout (docs/plan.md, phase 6): the markers of a reply, through
# the binary. The printer itself is held to the colour goldens in
# rust/tests/goldens.rs; these hold the flag to the stream. bats gives the
# binary a pipe, never a terminal, so `auto` is off here.

replay() { ASK_CLAUDE_REPLAY="$FIX/sse/$1.sse" "$BIN" "${@:2}"; }

# sgr <file>: the file carries an SGR sequence (ESC [)
sgr() { LC_ALL=C grep -q $'\033\\[' "$1"; }

@test "color: --color=always into a pipe prints the colour golden" {
    for f in thinking code-execution web-search mcp; do
        replay "$f" --color=always "ping" >"$TMP/out.txt" 2>/dev/null
        assert_golden "$FIX/sse/$f.stream.color.txt" "$TMP/out.txt"
        sgr "$TMP/out.txt"
    done
}

@test "color: --color=never prints the plain golden, with no SGR" {
    for f in thinking code-execution web-search mcp; do
        replay "$f" --color=never "ping" >"$TMP/out.txt" 2>"$TMP/err.txt"
        assert_golden "$FIX/sse/$f.stream.txt" "$TMP/out.txt"
        refute sgr "$TMP/out.txt"
        refute sgr "$TMP/err.txt"
    done
}

@test "color: --color=auto into a pipe is plain, and so is no flag" {
    replay thinking --color=auto "ping" >"$TMP/out.txt" 2>"$TMP/err.txt"
    assert_golden "$FIX/sse/thinking.stream.txt" "$TMP/out.txt"
    refute sgr "$TMP/err.txt"
    replay thinking "ping" >"$TMP/out.txt" 2>"$TMP/err.txt"
    assert_golden "$FIX/sse/thinking.stream.txt" "$TMP/out.txt"
    refute sgr "$TMP/err.txt"
}

@test "color: the reply's text is never coloured" {
    replay text --color=always "ping" >"$TMP/out.txt" 2>/dev/null
    assert_golden "$FIX/sse/text.stream.txt" "$TMP/out.txt"
}

@test "color: an escape in the reply is still removed under --color=always" {
    {
        printf 'data: %s\n\n' '{"type":"message_start","message":{"model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":0}}}'
        printf 'data: %s\n\n' '{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}'
        printf 'data: %s\n\n' '{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}'
        printf 'data: %s\n\n' '{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"a\u001b[31mred"}}'
        printf 'data: %s\n\n' '{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}'
        printf 'data: %s\n\n' '{"type":"message_stop"}'
    } >"$TMP/esc.sse"
    ASK_CLAUDE_REPLAY="$TMP/esc.sse" "$BIN" --color=always "ping" >"$TMP/out.txt" 2>/dev/null
    # the one span is the marker's; the reply's own escape is gone
    printf '\033[2m(thinking) \033[0ma[31mred\n' >"$TMP/expected.txt"
    assert_golden "$TMP/expected.txt" "$TMP/out.txt"
}

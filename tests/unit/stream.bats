#!/usr/bin/env bats
load helpers

# lib/stream.jq: `jq --unbuffered -rj` over event payloads -> terminal bytes.

stream() { events "$1" | jq --unbuffered -rj -f "$ROOT/lib/stream.jq"; }

@test "stream: text prints the reply and a final newline" {
    stream text > "$TMP/out.txt"
    assert_golden "$FIX/sse/text.stream.txt" "$TMP/out.txt"
    [ "$(cat "$TMP/out.txt")" = "pong" ]
}

@test "stream: thinking block shows a marker, not its content" {
    stream thinking > "$TMP/out.txt"
    assert_golden "$FIX/sse/thinking.stream.txt" "$TMP/out.txt"
    refute grep -q 'signature' "$TMP/out.txt"
}

@test "stream: code execution shows running and result markers" {
    stream code-execution > "$TMP/out.txt"
    assert_golden "$FIX/sse/code-execution.stream.txt" "$TMP/out.txt"
    grep -q '(running code) (result rc=0) 1024' "$TMP/out.txt"
}

@test "stream: ping and message_delta print nothing" {
    printf '%s\n' '{"type":"ping"}' '{"type":"message_delta","delta":{"stop_reason":"end_turn"}}' \
        | jq --unbuffered -rj -f "$ROOT/lib/stream.jq" > "$TMP/out.txt"
    [ ! -s "$TMP/out.txt" ]
}

@test "stream: control characters in the reply never reach the terminal" {
    # ESC ] 0 ; t BEL (title), ESC [ 2 K (erase line), CSI U+009B, CR, DEL;
    # tab and newline are kept
    printf '%s\n' '{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"a\u001b]0;t\u0007b\u001b[2Kc\u009b2Kd\re\u007ff\tg\nh"}}' \
        | jq --unbuffered -rj -f "$ROOT/lib/stream.jq" > "$TMP/out.txt"
    printf 'a]0;tb[2Kc2Kdef\tg\nh' > "$TMP/expected.txt"
    assert_golden "$TMP/expected.txt" "$TMP/out.txt"
    # nothing in 0x00-0x08, 0x0b-0x1f or 0x7f survives
    LC_ALL=C refute grep -q $'[\x01-\x08\x0b-\x1f\x7f]' "$TMP/out.txt"
}

@test "stream: web search, web fetch, dynamic filtering and MCP calls print their markers" {
    for f in web-search web-fetch web-dynamic mcp; do
        events "$f" | jq --unbuffered -rj -f "$ROOT/lib/stream.jq" > "$TMP/out.txt"
        assert_golden "$FIX/sse/$f.stream.txt" "$TMP/out.txt"
    done
}

@test "stream: an advisor call prints its marker, plaintext or encrypted advice alike" {
    for f in advisor advisor-redacted; do
        events "$f" | jq --unbuffered -rj -f "$ROOT/lib/stream.jq" > "$TMP/out.txt"
        assert_golden "$FIX/sse/$f.stream.txt" "$TMP/out.txt"
    done
    # an advisor error names its code and the reply goes on
    printf '%s\n' '{"type":"content_block_start","index":1,"content_block":{"type":"advisor_tool_result","tool_use_id":"srvtoolu_FIXTURE","content":{"type":"advisor_tool_result_error","error_code":"overloaded"}}}' |
        jq -rj -f "$ROOT/lib/stream.jq" | grep -qx '(advisor error: overloaded) '
}

# The nine bidirectional formatting characters (U+202A to U+202E, U+2066 to
# U+2069) reorder what the user reads and are dropped; the marks U+200E,
# U+200F and U+061C are ordinary right-to-left text and are kept. The
# characters are written as JSON escapes, never as invisible literals.
# no_bidi <file>: none of the nine, as UTF-8 bytes, is in the file (fixed
# strings: a byte range in a bracket is not portable across greps)
no_bidi() {
    refute grep -qF -e $'\xe2\x80\xaa' -e $'\xe2\x80\xab' -e $'\xe2\x80\xac' -e $'\xe2\x80\xad' \
        -e $'\xe2\x80\xae' -e $'\xe2\x81\xa6' -e $'\xe2\x81\xa7' -e $'\xe2\x81\xa8' -e $'\xe2\x81\xa9' "$1"
}

@test "stream: bidirectional formatting characters never reach the terminal, the marks do" {
    printf '%s\n' '{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"a\u202ab\u202bc\u202cd\u202de\u202ef\u2066g\u2067h\u2068i\u2069j"}}' \
        '{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"\u200ex\u200fy\u061cz"}}' \
        | jq --unbuffered -rj -f "$ROOT/lib/stream.jq" > "$TMP/out.txt"
    printf 'abcdefghij\xe2\x80\x8ex\xe2\x80\x8fy\xd8\x9cz' > "$TMP/expected.txt"
    assert_golden "$TMP/expected.txt" "$TMP/out.txt"
    no_bidi "$TMP/out.txt"
}

@test "stream: a reply through the binary prints no bidi formatting character, and keeps U+200F" {
    {
        printf 'data: %s\n\n' '{"type":"message_start","message":{"model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":0}}}'
        printf 'data: %s\n\n' '{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}'
        printf 'data: %s\n\n' '{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"a\u202eb\u2066c\u200fd"}}'
        printf 'data: %s\n\n' '{"type":"content_block_stop","index":0}'
        printf 'data: %s\n\n' '{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}'
        printf 'data: %s\n\n' '{"type":"message_stop"}'
    } > "$TMP/bidi.sse"
    ASK_CLAUDE_REPLAY="$TMP/bidi.sse" "$BIN" "ping" > "$TMP/out.txt" 2>/dev/null
    printf 'abc\xe2\x80\x8fd\n' > "$TMP/expected.txt"
    assert_golden "$TMP/expected.txt" "$TMP/out.txt"
}

@test "stream: an API error message through the binary prints no bidi formatting character" {
    printf '%s' '{"type":"error","error":{"type":"invalid_request_error","message":"bad \u202emodel\u2069 name\u200f"},"request_id":"req_FIXTURE"}' > "$TMP/error.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/error.sse" "$BIN" "ping"
    [ "$status" -eq 1 ]
    printf '%s' "$stderr" > "$TMP/err.txt"
    no_bidi "$TMP/err.txt"
    grep -q $'bad model name\xe2\x80\x8f' "$TMP/err.txt"
}

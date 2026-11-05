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
    ! grep -q 'signature' "$TMP/out.txt"
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
    ! LC_ALL=C grep -q $'[\x01-\x08\x0b-\x1f\x7f]' "$TMP/out.txt"
}

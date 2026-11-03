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

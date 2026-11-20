#!/usr/bin/env bats
load helpers

# lib/stream.jq one branch at a time, like reassemble-steps.bats. Each test
# feeds the printer only the event types its step handles and derives the
# expected bytes from those raw events, so a step turns green as soon as its
# own branch is written.
#
#   bats --filter-tags step:stream_text_delta tests/unit/stream-steps.bats
#
# Steps, in order: stream_content_block_start, stream_text_delta,
# stream_message_stop, stream_else. The goldens in stream.bats stay the gate
# for the finished program.

# pick <fixture> <type>...: the fixture's events of those types, one per line
pick() {
    local f="$1"
    shift
    events "$f" | jq -c 'select(.type as $t | $ARGS.positional | index($t))' --args "$@"
}

# print_events: run lib/stream.jq over events on stdin, the way claude.sh did
print_events() { jq --unbuffered -rj -f "$ROOT/lib/stream.jq"; }

# same_bytes <expected-file> <actual-file>
same_bytes() {
    cmp -s "$1" "$2" || {
        echo "expected: $(od -c "$1" | head -3)" >&2
        echo "actual:   $(od -c "$2" | head -3)" >&2
        return 1
    }
}

# --- content_block_start -----------------------------------------------------

# bats test_tags=step:stream_content_block_start
@test "step stream_content_block_start: a marker per non-text block, nothing for text" {
    for f in text thinking code-execution; do
        pick "$f" content_block_start | print_events > "$TMP/out"
        pick "$f" content_block_start | jq -rj '.content_block
            | if .type == "thinking" then "(thinking) "
              elif .type == "server_tool_use" then "(running code) "
              elif .type == "bash_code_execution_tool_result" then
                  "(result rc=\(.content.return_code // "?")) "
              else "" end' > "$TMP/expected"
        same_bytes "$TMP/expected" "$TMP/out"
    done
}

# bats test_tags=step:stream_content_block_start
@test "step stream_content_block_start: a missing return code prints rc=?" {
    printf '%s\n' '{"type":"content_block_start","index":0,"content_block":{"type":"bash_code_execution_tool_result","content":{}}}' \
        | print_events > "$TMP/out"
    printf '(result rc=?) ' > "$TMP/expected"
    same_bytes "$TMP/expected" "$TMP/out"
}

# --- text_delta --------------------------------------------------------------

# bats test_tags=step:stream_text_delta
@test "step stream_text_delta: text deltas print their text, in order" {
    for f in text thinking code-execution; do
        pick "$f" content_block_delta | print_events > "$TMP/out"
        pick "$f" content_block_delta \
            | jq -rj 'if .delta.type == "text_delta" then .delta.text else "" end' > "$TMP/expected"
        same_bytes "$TMP/expected" "$TMP/out"
    done
}

# --- message_stop ------------------------------------------------------------

# bats test_tags=step:stream_message_stop
@test "step stream_message_stop: the stream ends with one newline" {
    pick text message_stop | print_events > "$TMP/out"
    printf '\n' > "$TMP/expected"
    same_bytes "$TMP/expected" "$TMP/out"
}

# --- else --------------------------------------------------------------------

# bats test_tags=step:stream_else
@test "step stream_else: every other event prints nothing" {
    for f in text thinking code-execution; do
        {
            pick "$f" ping message_start message_delta content_block_stop
            pick "$f" content_block_delta | jq -c 'select(.delta.type != "text_delta")'
        } | print_events > "$TMP/out"
        [ ! -s "$TMP/out" ] || {
            echo "printed: $(od -c "$TMP/out" | head -3)" >&2
            return 1
        }
    done
}

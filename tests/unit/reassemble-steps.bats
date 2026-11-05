#!/usr/bin/env bats
load helpers

# lib/reassemble.jq one branch at a time. Each test feeds the reassembler only
# the event types its step handles (plus the ones it builds on) and derives the
# expected values from those raw events, so a step turns green as soon as its
# own branch is written, whatever the later branches still do. Block checks go
# through `view`, which reads .blocks before the FINISH step and .content after
# it, so earlier steps stay green once FINISH lands.
#
#   just step message_start      # one step: bats --filter-tags step:<name>
#
# Steps, in order: message_start, content_block_start, content_block_delta,
# content_block_stop, message_delta, error, finish. The byte-for-byte goldens
# in reassemble.bats stay the gate for the finished program.

# pick <fixture> <type>...: the fixture's events of those types, one per line
pick() {
    local f="$1"
    shift
    events "$f" | jq -c 'select(.type as $t | $ARGS.positional | index($t))' --args "$@"
}

# fold <fixture> <type>...: run lib/reassemble.jq over just those events
fold() { pick "$@" | jq -s -f "$ROOT/lib/reassemble.jq"; }

# check <jq-expr>: true for $out (the fold) against $ev (the picked events)
check() {
    jq -e -n --argjson out "$out" --argjson ev "$ev" '
        def view: if has("blocks") then .blocks
                  else (.content // []) | to_entries
                       | map({key: (.key | tostring), value}) | from_entries end;
        def starts: [$ev[] | select(.type == "content_block_start")];
        def tool: .content_block.type | IN("tool_use", "server_tool_use");
        '"$1" >/dev/null || {
        echo "check failed: $1" >&2
        echo "out: $out" >&2
        return 1
    }
}

# setup_fold <fixture> <type>...: sets $out and $ev for check
setup_fold() {
    out="$(fold "$@")"
    ev="$(pick "$@" | jq -s .)"
}

# --- message_start -----------------------------------------------------------

# bats test_tags=step:message_start
@test "step message_start: model, usage and container come from .message" {
    for f in text code-execution; do
        setup_fold "$f" message_start
        check '$ev[0].message as $m
               | $out.model == $m.model and $out.usage == $m.usage
                 and $out.container == $m.container'
    done
}

# --- content_block_start -----------------------------------------------------

# bats test_tags=step:content_block_start
@test "step content_block_start: each block is stored under its index" {
    for f in text thinking code-execution; do
        setup_fold "$f" message_start content_block_start
        check '($out | view | length) == (starts | length)
               and all(starts[]; ($out | view)[.index | tostring] as $b
                   | ($b | del(.partial)) == .content_block)'
    done
}

# bats test_tags=step:content_block_start
@test "step content_block_start: tool blocks get partial \"\", others none" {
    setup_fold code-execution message_start content_block_start
    check 'all(starts[]; ($out | view)[.index | tostring] as $b
               | if tool then $b.partial == "" else ($b | has("partial") | not) end)'
}

# --- content_block_delta -----------------------------------------------------

# bats test_tags=step:content_block_delta
@test "step content_block_delta: text, thinking, signature and partial accumulate per block" {
    for f in text thinking code-execution; do
        setup_fold "$f" message_start content_block_start content_block_delta
        check 'all(starts[]; . as $s | ($out | view)[$s.index | tostring] as $b
               | [$ev[] | select(.type == "content_block_delta" and .index == $s.index) | .delta] as $ds
               | ([$ds[] | select(.type == "text_delta") | .text]
                  | length == 0 or $b.text == ($s.content_block.text // "") + add)
               and ([$ds[] | select(.type == "thinking_delta") | .thinking]
                  | length == 0 or $b.thinking == ($s.content_block.thinking // "") + add)
               and ([$ds[] | select(.type == "signature_delta") | .signature]
                  | length == 0 or $b.signature == last)
               and ([$ds[] | select(.type == "input_json_delta") | .partial_json]
                  | length == 0 or $b.partial == add))'
    done
}

# --- content_block_stop ------------------------------------------------------

# bats test_tags=step:content_block_stop
@test "step content_block_stop: tool input is parsed from partial, partial dropped" {
    setup_fold code-execution message_start content_block_start content_block_delta content_block_stop
    check 'all(starts[]; . as $s | ($out | view)[$s.index | tostring] as $b
           | ($b | has("partial") | not)
           and (if tool then
                  [$ev[] | select(.type == "content_block_delta" and .index == $s.index)
                         | .delta | select(.type == "input_json_delta") | .partial_json]
                  | add // "" | (if . == "" then {} else fromjson end) == $b.input
                else true end))'
}

# --- message_delta -----------------------------------------------------------

# bats test_tags=step:message_delta
@test "step message_delta: stop reason, stop details, merged usage, container" {
    for f in text refusal pause-turn code-execution; do
        setup_fold "$f" message_start message_delta
        check '($ev[] | select(.type == "message_start") | .message) as $m
               | ($ev[] | select(.type == "message_delta")) as $d
               | $out.stop_reason == $d.delta.stop_reason
                 and $out.stop_details == $d.delta.stop_details
                 and $out.usage == ($m.usage + ($d.usage // {}))
                 and $out.container == ($d.delta.container // $m.container)'
    done
}

# --- error -------------------------------------------------------------------

# bats test_tags=step:error
@test "step error: the error object is kept" {
    # error.sse is a bare JSON error body (HTTP 404), not an SSE stream
    ev="$(jq -c . "$FIX/sse/error.sse" | jq -s .)"
    out="$(jq -c . "$FIX/sse/error.sse" | jq -s -f "$ROOT/lib/reassemble.jq")"
    check '$out.error == $ev[0].error'
}

# --- finish ------------------------------------------------------------------

# bats test_tags=step:finish
@test "step finish: .blocks becomes .content in index order" {
    for f in thinking code-execution; do
        setup_fold "$f" message_start content_block_start content_block_delta content_block_stop message_delta
        check '($out | has("blocks") | not)
               and ([$out.content[].type] == [starts | sort_by(.index)[] | .content_block.type])'
    done
}

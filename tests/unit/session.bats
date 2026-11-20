#!/usr/bin/env bats
load helpers

# lib/session.jq prints a Managed Agents event stream as it arrives (like
# lib/stream.jq); lib/session-fold.jq folds one stream into what the shell
# decides on (like lib/reassemble.jq). Goldens in tests/fixtures/sessions.

STREAMS="text budget error confirm/1 confirm/2 custom-tool/1 custom-tool/2 custom-tool/3"
events() { sed -n 's/^data: //p' "$FIX/sessions/$1.sse"; }

@test "session: the printer matches every stream golden" {
    for f in $STREAMS; do
        events "$f" | jq --unbuffered -rj -f "$ROOT/lib/session.jq" > "$TMP/out.txt"
        assert_golden "$FIX/sessions/$f.stream.txt" "$TMP/out.txt"
    done
}

@test "session: the folder matches every fold golden" {
    for f in $STREAMS; do
        events "$f" | jq -s -f "$ROOT/lib/session-fold.jq" > "$TMP/out.json"
        assert_golden "$FIX/sessions/$f.fold.json" "$TMP/out.json"
    done
}

@test "session: an unknown event type prints nothing and folds to nothing" {
    printf '{"type":"session.thread_created","id":"x"}\n{"type":"workflow_run.created","id":"y"}\n' > "$TMP/in.jsonl"
    [ -z "$(jq -rj -f "$ROOT/lib/session.jq" "$TMP/in.jsonl")" ]
    jq -s -f "$ROOT/lib/session-fold.jq" "$TMP/in.jsonl" | jq -e '.stop_reason==null and .event_ids==[] and .asks==[] and .error==null'
}

@test "session: a stream without session.usage folds to zero usage, not null" {
    events text | grep -v '"type":"session.usage"' | jq -s -f "$ROOT/lib/session-fold.jq" | jq -e '.usage.input_tokens==0 and .usage.list_cost=="0" and .usage.active_seconds==0'
}

@test "session: control characters in an agent message never reach the terminal" {
    printf '{"type":"agent.message","content":[{"type":"text","text":"a\\u001b]0;owned\\u0007b\\u009bc\\td\\n"}]}\n' |
        jq -rj -f "$ROOT/lib/session.jq" > "$TMP/out.txt"
    printf 'a]0;ownedbc\td\n' > "$TMP/want.txt"
    assert_golden "$TMP/want.txt" "$TMP/out.txt"
}

@test "session: tool markers name the tool, idle ends the line" {
    printf '{"type":"agent.tool_use","name":"bash","input":{"command":"ls"}}\n{"type":"agent.mcp_tool_use","name":"linear","input":{}}\n{"type":"session.status_idle","stop_reason":{"type":"end_turn"}}\n' |
        jq -rj -f "$ROOT/lib/session.jq" > "$TMP/out.txt"
    printf '(bash) (mcp linear) \n' > "$TMP/want.txt"
    assert_golden "$TMP/want.txt" "$TMP/out.txt"
}

@test "session: a turn with no session.error or session.usage still folds (jq 1.8: last(empty) is empty)" {
    run jq -s -c -f "$ROOT/lib/session-fold.jq" <<<'{"type":"session.status_idle","stop_reason":{"type":"end_turn"}}'
    [ "$status" -eq 0 ]
    jq -e '.stop_reason=="end_turn" and .error==null and .usage.input_tokens==0' <<<"$output"
    run jq -s -c -f "$ROOT/lib/session-fold.jq" </dev/null
    [ "$status" -eq 0 ]
    jq -e '.stop_reason==null and .asks==[]' <<<"$output"
}

# U+202E (RLO) and U+2069 (PDI) reorder what the user reads and are dropped;
# U+200F (RLM) is ordinary right-to-left text and is kept (KTD16)
@test "session: bidirectional formatting characters in an agent message never reach the terminal" {
    printf '{"type":"agent.message","content":[{"type":"text","text":"a\\u202eb\\u2066c\\u2069d\\u200fe"}]}\n' > "$TMP/in.jsonl"
    printf 'abcd\xe2\x80\x8fe' > "$TMP/expected.txt"
    jq -rj -f "$ROOT/lib/session.jq" "$TMP/in.jsonl" > "$TMP/out.txt"
    assert_golden "$TMP/expected.txt" "$TMP/out.txt"
}

@test "session: the history drops bidirectional formatting characters and keeps the marks" {
    printf '{"data":[{"type":"user.message","content":[{"type":"text","text":"x\\u202ay"}]},{"type":"agent.message","content":[{"type":"text","text":"a\\u202eb\\u2066c\\u200fd"}]}]}\n' > "$TMP/page.json"
    printf 'you: xy\n\nagent: abc\xe2\x80\x8fd\n\n' > "$TMP/expected.txt"
    jq -r -f "$ROOT/lib/session-history.jq" "$TMP/page.json" > "$TMP/out.txt"
    assert_golden "$TMP/expected.txt" "$TMP/out.txt"
}

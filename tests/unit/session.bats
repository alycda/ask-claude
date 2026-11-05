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

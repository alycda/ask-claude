#!/usr/bin/env bats
load helpers

# present <file>: the fixture exists and is not empty, else say which one is
# missing. A bare `[ -s ]` fails without a name, and the first guess is then
# that the test is wrong; the fixtures are gitignored by pattern with tracked
# exceptions, so a missing one is usually an exception that was not added.
present() {
    [ -s "$1" ] || { echo "missing fixture: ${1#"$ROOT/"}" >&2; return 1; }
}

@test "harness: fixtures are present and parse" {
    for f in text thinking code-execution pause-turn refusal web-search web-fetch mcp advisor advisor-redacted; do
        present "$FIX/sse/$f.sse"
        events "$f" | jq -e 'select(.type=="message_stop")' >/dev/null
    done
    jq -e '.error.type == "not_found_error"' "$FIX/sse/error.sse"
}

@test "harness: tools under test are on PATH" {
    command -v jq
    command -v usage
    command -v bats
}

# Session fixtures (docs/plans/2026-11-05-001): one file per stream, ids in
# the FIXTURE form tests/record-session.sh writes. text, confirm/1-2,
# custom-tool/1-3, chat/1-3 and files/1 are live recordings, made with
# tests/record-session.sh against the agents its header describes, and the
# goldens are generated from them. budget.sse and error.sse are hand-derived
# from text.sse. not-sse/1, terminated/1 and waiting-unknown/1 are still
# reconstructed by hand, with a live stream's framing; they were not part of
# the re-recording. Every stream ends in an idle or a termination, because
# that is where ask-claude stops reading.
sessions() { sed -n 's/^data: //p' "$FIX/sessions/$1.sse"; }

@test "harness: session fixtures parse and end in idle or terminated" {
    for f in text budget error confirm/1 confirm/2 custom-tool/1 custom-tool/2 custom-tool/3; do
        present "$FIX/sessions/$f.sse"
        sessions "$f" | jq -e 'select(.type=="session.status_idle" or .type=="session.status_terminated")' >/dev/null
        last=$(sessions "$f" | tail -n 1 | jq -r .type)
        [ "$last" = session.status_idle ] || [ "$last" = session.status_terminated ]
        [ -f "$FIX/sessions/$f.stream.txt" ]
        jq -e 'has("stop_reason") and has("event_ids") and has("asks") and has("custom") and has("usage") and has("error")' "$FIX/sessions/$f.fold.json" >/dev/null
    done
}

@test "harness: a confirmation's event id is the tool_use that was answered" {
    id=$(jq -r '.event_ids[0]' "$FIX/sessions/confirm/1.fold.json")
    [ "$id" = "$(jq -r '.asks[0].id' "$FIX/sessions/confirm/1.fold.json")" ]
    sessions confirm/2 | jq -e --arg id "$id" 'select(.type=="agent.tool_result" and .tool_use_id==$id)' >/dev/null
}

#!/usr/bin/env bats
load helpers

@test "harness: fixtures are present and parse" {
    for f in text thinking code-execution pause-turn refusal; do
        [ -s "$FIX/sse/$f.sse" ]
        events "$f" | jq -e 'select(.type=="message_stop")' >/dev/null
    done
    jq -e '.error.type == "not_found_error"' "$FIX/sse/error.sse"
}

@test "harness: tools under test are on PATH" {
    command -v jq
    command -v usage
    command -v bats
}

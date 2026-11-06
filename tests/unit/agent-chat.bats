#!/usr/bin/env bats
load helpers

# `ask-claude agent` with no prompt: a chat on a session; `agent resume`,
# `agent list`. The chat/ fixture directory holds one stream per turn
# (1.sse, 2.sse, 3.sse), a sessions page (sessions.json) and an events page
# (events.json) that replay stands in for GET /v1/sessions and
# GET /v1/sessions/{id}/events.

export ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE
chat() { # stdin script, then args
    printf "$1" | ASK_CLAUDE_REPLAY="$FIX/sessions/chat" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent "${@:2}"
}
request() { sed -n "$1p" "$TMP/requests.jsonl"; }
requests() { wc -l < "$TMP/requests.jsonl" 2>/dev/null || echo 0; }
STATE_DIR() { echo "$XDG_STATE_HOME/ask-claude"; }

@test "chat: the first line creates the session with it, /quit says how to resume" {
    run --separate-stderr chat 'hi\n/quit\n'
    [ "$status" -eq 0 ]
    grep -q '^pong' <<<"$output"
    [ "$(requests)" -eq 1 ]
    request 1 | jq -e '.initial_events[0].content[0].text=="hi"'
    grep -q 'resume with: ask-claude agent resume sesn_FIXTURE01' <<<"$stderr"
    [ -f "$(STATE_DIR)/sessions/sesn_FIXTURE01" ]
}

@test "chat: later lines are user.message events on the same session" {
    run --separate-stderr chat 'hi\nagain\n/quit\n'
    [ "$status" -eq 0 ]
    [ "$(grep -c '^pong' <<<"$output")" -eq 2 ]
    [ "$(requests)" -eq 2 ]
    request 2 | jq -e '.events==[{"type":"user.message","content":[{"type":"text","text":"again"}]}]'
    [ "$(grep -c '^\[sesn_FIXTURE01 ' <<<"$stderr")" -eq 2 ]
}

@test "chat: quitting before a line creates nothing" {
    run --separate-stderr chat '/quit\n'
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 0 ]
    [ -z "$(ls "$(STATE_DIR)"/sessions 2>/dev/null)" ]
    refute grep -q 'resume with' <<<"$stderr"
}

@test "chat: blank lines are ignored" {
    run --separate-stderr chat '\n\n/quit\n'
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 0 ]
}

@test "chat: /history prints the session's messages from the API" {
    run --separate-stderr chat 'hi\n/history\n/quit\n'
    [ "$status" -eq 0 ]
    grep -q '^you: Reply with the single word: pong' <<<"$output"
    grep -q '^agent: pong' <<<"$output"
}

@test "chat: /new starts a second session" {
    run --separate-stderr chat 'hi\n/new\nhi\n/quit\n'
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 2 ]
    request 2 | jq -e 'has("initial_events")'
    grep -q 'new session' <<<"$stderr"
}

@test "resume: by id, prints the session so far, then takes turns" {
    run --separate-stderr chat 'again\n/quit\n' resume sesn_FIXTURE01
    [ "$status" -eq 0 ]
    grep -q '^resuming sesn_FIXTURE01' <<<"$stderr"
    grep -q '^you: Reply with the single word: pong' <<<"$stderr"
    [ "$(requests)" -eq 1 ]
    request 1 | jq -e '.events[0].content[0].text=="again"'
}

@test "resume: a unique prefix of a session started here" {
    mkdir -p "$(STATE_DIR)/sessions"
    printf 'agent=agent_FIXTURE\nenvironment=env_FIXTURE\ncreated=20260101T000000\n' > "$(STATE_DIR)/sessions/sesn_FIXTURE01"
    printf 'agent=agent_FIXTURE\nenvironment=env_FIXTURE\ncreated=20260101T000000\n' > "$(STATE_DIR)/sessions/sesn_FIXTURE02"
    run --separate-stderr chat '/quit\n' resume sesn_FIXTURE01
    [ "$status" -eq 0 ]
    run --separate-stderr chat '/quit\n' resume FIXTURE0
    [ "$status" -eq 2 ]
    grep -q 'matches 2' <<<"$stderr"
    run --separate-stderr chat '/quit\n' resume nope
    [ "$status" -eq 2 ]
    grep -q 'nothing to resume' <<<"$stderr"
}

@test "list: the API's sessions, newest first, id status cost title" {
    run --separate-stderr chat '' list
    [ "$status" -eq 0 ]
    [ "${lines[0]%% *}" = "sesn_FIXTURE01" ]
    grep -q 'fixture custom tool' <<<"${lines[0]}"
    grep -q 'idle' <<<"${lines[0]}"
    grep -qE '\$0\.00' <<<"${lines[0]}"
    [ "${#lines[@]}" -eq 3 ]
    [ "$(requests)" -eq 0 ]
}

@test "list: no sessions prints nothing, exit 0" {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sessions/text.sse" "$BIN" agent list
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "history: lib/session-history.jq matches its golden" {
    jq -r -f "$ROOT/lib/session-history.jq" "$FIX/sessions/chat/events.json" > "$TMP/out.txt"
    assert_golden "$FIX/sessions/chat/history.txt" "$TMP/out.txt"
}

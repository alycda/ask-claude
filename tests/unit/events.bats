#!/usr/bin/env bats
# shellcheck disable=SC2016  # jq programs in single quotes mention $ on purpose
load helpers

# The event log: $XDG_STATE_HOME/ask-claude/events.jsonl, one JSON line per
# request (lib/event.jq builds the line; tests/unit/event.bats has its goldens).
# These run the binary under test against recorded fixtures.

replay() { ASK_CLAUDE_REPLAY="$FIX/sse/$1.sse" "$BIN" "${@:2}"; }
LOG() { echo "$XDG_STATE_HOME/ask-claude/events.jsonl"; }
history_name() { basename "$(ls "$XDG_STATE_HOME"/ask-claude/*.json)" .json; }

# ---- Messages ------------------------------------------------------------

@test "events: a one-shot writes one line, and never the prompt" {
    run --separate-stderr replay text "my secret prompt"
    [ "$status" -eq 0 ]
    [ "$(wc -l <"$(LOG)")" -eq 1 ]
    jq -e '.kind=="messages" and .model=="claude-opus-5-5" and .stop_reason=="end_turn" and .round==0 and .error==null' "$(LOG)"
    jq -e --arg c "$(history_name)" '.chat==$c' "$(LOG)"
    jq -e '.host=="api.anthropic.com" and .usage_scope=="request" and .input_tokens==18' "$(LOG)"
    refute grep -q 'secret' "$(LOG)"
}

@test "events: the timestamp is UTC ISO 8601 and the version is the script's" {
    replay text "ping" >/dev/null 2>&1
    jq -e '.ts | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")' "$(LOG)"
    jq -e --arg v "$("$BIN" --version | cut -d' ' -f2)" '.ask_claude==$v and .v==1' "$(LOG)"
}

@test "events: the host is the --base-url's, without its path" {
    replay text --base-url http://localhost:8080/some/path "ping" >/dev/null 2>&1
    jq -e '.host=="localhost:8080"' "$(LOG)"
}

@test "events: each pause_turn round is its own line, numbered from 0" {
    run --separate-stderr replay pause-turn "ping"
    # the single-file replay answers pause_turn every round: rounds 0..8
    [ "$(wc -l <"$(LOG)")" -eq "$(($(grep -c 'pause_turn: continuing' <<<"$stderr") + 1))" ]
    [ "$(jq -s -c 'map(.round)' "$(LOG)")" = "[0,1,2,3,4,5,6,7,8]" ]
    jq -s -e 'all(.stop_reason=="pause_turn")' "$(LOG)"
}

@test "events: an advisor reply carries the advisor's usage" {
    replay advisor --advisor claude-sonnet-5 "ping" >/dev/null 2>&1
    jq -e '.advisor.model=="claude-sonnet-5" and .advisor.calls==1 and .advisor.input_tokens==2319' "$(LOG)"
}

@test "events: without the advisor, advisor is null" {
    replay text "ping" >/dev/null 2>&1
    jq -e '.advisor==null' "$(LOG)"
}

@test "events: a refusal carries its category" {
    replay refusal "ping" >/dev/null 2>&1
    jq -e '.stop_reason=="refusal" and .stop_category=="cyber"' "$(LOG)"
}

@test "events: an API error is a line with its type and none of its message" {
    run --separate-stderr replay error "ping"
    [ "$status" -eq 1 ]
    [ "$(wc -l <"$(LOG)")" -eq 1 ]
    jq -e '.error=="not_found_error" and .input_tokens==null' "$(LOG)"
    refute grep -q 'does-not-exist' "$(LOG)"
}

@test "events: no response is a no_response line" {
    : >"$TMP/empty.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/empty.sse" "$BIN" "ping"
    [ "$status" -eq 1 ]
    jq -e '.error=="no_response"' "$(LOG)"
}

@test "events: a body that is not JSON is an unreadable line" {
    printf 'Bad Gateway\n' >"$TMP/html.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/html.sse" "$BIN" "ping"
    [ "$status" -eq 1 ]
    jq -e '.error=="unreadable"' "$(LOG)"
}

@test "events: a stream jq cannot fold is a parse line" {
    printf 'data: {"type":"message_start"\n\n' >"$TMP/broken.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/broken.sse" "$BIN" "ping"
    [ "$status" -eq 1 ]
    jq -e '.error=="parse"' "$(LOG)"
}

@test "events: ASK_CLAUDE_EVENTS=off writes nothing and creates no file" {
    ASK_CLAUDE_EVENTS=off replay text "ping" >/dev/null 2>&1
    [ ! -e "$(LOG)" ]
}

@test "events: a log that cannot be written changes nothing the user sees" {
    replay text "ping" >"$TMP/out.0" 2>"$TMP/err.0"
    s0=$?
    rm -rf "$XDG_STATE_HOME"
    mkdir -p "$(LOG)" # a directory where the file should be
    replay text "ping" >"$TMP/out.1" 2>"$TMP/err.1"
    s1=$?
    [ "$s0" -eq "$s1" ]
    diff "$TMP/out.0" "$TMP/out.1"
    diff <(sed 's/ [0-9]*s\]$/]/' "$TMP/err.0") <(sed 's/ [0-9]*s\]$/]/' "$TMP/err.1")
}

@test "events: the log is private whatever the umask" {
    (umask 022 && replay text "ping" >/dev/null 2>&1)
    [ "$(stat -c %a "$(LOG)" 2>/dev/null || stat -f %Lp "$(LOG)")" = 600 ]
}

@test "events: a second run appends and leaves the first line alone" {
    replay text "one" >/dev/null 2>&1
    first=$(head -1 "$(LOG)")
    replay text "two" >/dev/null 2>&1
    [ "$(wc -l <"$(LOG)")" -eq 2 ]
    [ "$(head -1 "$(LOG)")" = "$first" ]
}

@test "events: list and resume do not see the log" {
    replay text "ping" >/dev/null 2>&1
    [ -s "$(LOG)" ]
    run --separate-stderr "$BIN" list
    [ "$status" -eq 0 ]
    [ "$(wc -l <<<"$output")" -eq 1 ]
    refute grep -q events <<<"$output"
}

# ---- Managed Agents sessions ---------------------------------------------

agent() { ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE ASK_CLAUDE_REPLAY="$FIX/sessions/$1" "$BIN" agent "${@:2}"; }

@test "events: a session turn writes one session line" {
    run --separate-stderr agent text.sse "my secret prompt" </dev/null
    [ "$status" -eq 0 ]
    [ "$(wc -l <"$(LOG)")" -eq 1 ]
    jq -e '.kind=="session" and .usage_scope=="session" and (.chat|startswith("sesn_")) and .stop_reason=="end_turn"' "$(LOG)"
    jq -e '.list_cost=="0" and .cache_write==4311 and .model==null and .error==null' "$(LOG)"
    refute grep -q 'secret' "$(LOG)"
}

@test "events: a session chat writes a line per turn, usage only grows" {
    run --separate-stderr env ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE \
        ASK_CLAUDE_REPLAY="$FIX/sessions/chat" "$BIN" agent <<<$'hi\nagain\n/quit'
    [ "$(wc -l <"$(LOG)")" -ge 2 ]
    jq -s -e 'all(.kind=="session") and ([.[].chat]|unique|length)==1' "$(LOG)"
    jq -s -e '[.[].input_tokens] as $t | [range(1; $t|length) | $t[.] >= $t[.-1]] | all' "$(LOG)"
}

@test "events: a session line counts the custom-tool rounds answered" {
    run --separate-stderr agent custom-tool "ping" </dev/null
    [ "$status" -eq 0 ]
    [ "$(wc -l <"$(LOG)")" -eq 1 ]
    jq -e '.round==2' "$(LOG)"
}

@test "events: a session error is a line with its type and none of its message" {
    run --separate-stderr agent error.sse "ping" </dev/null
    [ "$status" -eq 1 ]
    [ "$(wc -l <"$(LOG)")" -eq 1 ]
    jq -e '.kind=="session" and .error=="api_error"' "$(LOG)"
    refute grep -q 'could not be started' "$(LOG)"
}

@test "events: a wait this version cannot answer is an unanswerable line" {
    run --separate-stderr agent waiting-unknown "ping" </dev/null
    [ "$status" -eq 1 ]
    jq -e '.error=="unanswerable"' "$(LOG)"
}

@test "events: a terminated session is a normal line with its stop reason" {
    run --separate-stderr agent terminated "ping" </dev/null
    [ "$status" -eq 1 ]
    [ "$(wc -l <"$(LOG)")" -eq 1 ]
    jq -e '.stop_reason=="terminated" and .error==null' "$(LOG)"
}

@test "events: a refused events POST is a line with the API's error type" {
    run --separate-stderr env ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE \
        ASK_CLAUDE_REPLAY="$FIX/sessions/events-refused" "$BIN" agent resume sesn_FIXTURE01 <<<$'again\n/quit\n'
    jq -e '.kind=="session" and .error=="invalid_request_error"' "$(LOG)"
    refute grep -q 'archived' "$(LOG)"
}

@test "events: a session stream that never came is a no_response line" {
    mkdir -p "$TMP/none"
    run --separate-stderr env ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE \
        ASK_CLAUDE_REPLAY="$TMP/none" "$BIN" agent "ping" </dev/null
    [ "$status" -eq 1 ]
    jq -e '.kind=="session" and .error=="no_response"' "$(LOG)"
}

@test "events: ASK_CLAUDE_EVENTS=off writes nothing on the session path" {
    ASK_CLAUDE_EVENTS=off agent text.sse "ping" </dev/null >/dev/null 2>&1
    [ ! -e "$(LOG)" ]
}

#!/usr/bin/env bats
# shellcheck disable=SC2016  # jq programs in single quotes mention $ on purpose
load helpers

# `agent --local-mcp <command>`: a stdio MCP server on this machine answers the
# agent's custom tools. The session is the custom-tool recording: turn 1 and
# turn 2 each call get_weather {"city":"Paris"}, turn 3 ends. The server is
# tests/fixtures/mcp/fake-server, which logs every line it receives.

export ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE
SERVER="$FIX/mcp/fake-server"
replay() {
    ASK_CLAUDE_REPLAY="$FIX/sessions/custom-tool" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        FAKE_MCP_LOG="$TMP/mcp.log" "$BIN" agent "$@"
}
request() { sed -n "$1p" "$TMP/requests.jsonl"; }
# received <method>: how many lines with that method the server got
received() { jq -c --arg m "$1" 'select(.method == $m)' "$TMP/mcp.log" | wc -l | tr -d ' '; }
# result <n>: the first event of request <n>, the answer to a tool call
result() { request "$1" | jq -c '.events[0]'; }

@test "local-mcp: an allowed custom tool call runs on the local server and its result goes back" {
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    [ "$(wc -l <"$TMP/requests.jsonl")" -eq 3 ]
    result 2 | jq -e '.type == "user.custom_tool_result" and .custom_tool_use_id == "sevt_FIXTURE04"'
    result 2 | jq -e '.is_error == false and .content == [{type: "text", text: "Sunny in Paris"}]'
    result 3 | jq -e '.custom_tool_use_id == "sevt_FIXTURE13" and .is_error == false'
    jq -e 'select(.method == "tools/call") | .id == "sevt_FIXTURE04" and .params == {name: "get_weather", arguments: {city: "Paris"}}' "$TMP/mcp.log" | head -1 | grep -q true
    grep -q 'allowed local get_weather' <<<"$stderr"
}

@test "local-mcp: one server answers every round: one handshake, two calls" {
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    [ "$(received initialize)" -eq 1 ]
    [ "$(received notifications/initialized)" -eq 1 ]
    [ "$(received tools/list)" -eq 1 ]
    [ "$(received tools/call)" -eq 2 ]
}

@test "local-mcp: ASK_CLAUDE_LOCAL_MCP names the server like the flag" {
    ASK_CLAUDE_LOCAL_MCP="$SERVER" ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == false'
}

@test "local-mcp: without a terminal the call is denied and never reaches the server" {
    run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == true and (.content[0].text | test("denied"))'
    [ "$(received tools/call)" -eq 0 ]
    grep -q 'denied local get_weather' <<<"$stderr"
}

@test "local-mcp: ASK_CLAUDE_CONFIRM=deny denies the call" {
    ASK_CLAUDE_CONFIRM=deny run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == true'
    [ "$(received tools/call)" -eq 0 ]
}

@test "local-mcp: a tool the server does not list gets the unavailable error, not a call" {
    FAKE_MCP_TOOLS='["something_else"]' ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == true and (.content[0].text | test("no tool named get_weather"))'
    [ "$(received tools/call)" -eq 0 ]
}

@test "local-mcp: a tool result with isError goes back as an error, with its text" {
    FAKE_MCP_MODE=error-result ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == true and .content == [{type: "text", text: "no weather for Paris"}]'
}

@test "local-mcp: an embedded text resource goes back as text; binary content and links are named" {
    FAKE_MCP_MODE=mixed ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == false and (.content | length) == 5'
    result 2 | jq -e '.content[0].text == "downloaded"'
    result 2 | jq -e '.content[1].text == "Resource repo://o/r/contents/VERSION:\n0.2.7\n"'
    result 2 | jq -e '.content[2].text | test("binary resource repo://o/r/contents/logo.png \\(image/png\\) not passed on")'
    result 2 | jq -e '.content[3].text == "Resource link: repo://o/r/contents/README.md (README.md)"'
    result 2 | jq -e '.content[4].text | test("image content not passed on")'
    result 2 | jq -e 'all(.content[]; .type == "text")'
}

@test "local-mcp: a JSON-RPC error goes back as an error naming the code" {
    FAKE_MCP_MODE=rpc-error ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == true and (.content[0].text | test("-32602") and test("bad arguments"))'
}

@test "local-mcp: notifications, server requests and junk lines are skipped; the request gets method-not-found" {
    FAKE_MCP_MODE=noisy ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == false and .content[0].text == "Sunny in Paris"'
    jq -e 'select(.id == "srv-1") | .error.code == -32601' "$TMP/mcp.log" | head -1 | grep -q true
}

@test "local-mcp: a server that dies mid-call gives an error result, and the turn goes on" {
    FAKE_MCP_MODE=crash-on-call ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == true and (.content[0].text | test("exited"))'
    result 3 | jq -e '.is_error == true and (.content[0].text | test("exited"))'
}

@test "local-mcp: a server that does not answer in time gives an error result" {
    FAKE_MCP_MODE=hang-on-call ASK_CLAUDE_LOCAL_MCP_TIMEOUT=1 ASK_CLAUDE_CONFIRM=allow \
        run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == true and (.content[0].text | test("did not answer"))'
}

@test "local-mcp: a server that fails the handshake exits 2 before any request" {
    FAKE_MCP_MODE=no-init run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 2 ]
    grep -q 'initialize' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "local-mcp: a command that does not exist exits 2 before any request" {
    run --separate-stderr replay --local-mcp "$TMP/no-such-server" "weather?" </dev/null
    [ "$status" -eq 2 ]
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "local-mcp: the server is gone when the script exits" {
    FAKE_MCP_PIDFILE="$TMP/pid" ASK_CLAUDE_CONFIRM=allow run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    pid=$(cat "$TMP/pid")
    sleep 0.2
    run ! kill -0 "$pid"
}

# gone <pid>...: none of these processes is still running. A survivor is
# named and killed, every one of them, so a failure does not leave a process
# holding bats' descriptors (bats waits for those, and the test would hang).
gone() {
    local p alive=0
    for p in "$@"; do
        # a zombie is gone: it waits only for a parent to reap it, and an
        # orphan's parent is PID 1, which in a container may never reap
        if kill -0 "$p" 2>/dev/null && [[ $(ps -o stat= -p "$p") != Z* ]]; then
            echo "still running: $(ps -o pid=,args= -p "$p")" >&2
            kill -KILL "$p" 2>/dev/null
            alive=1
        fi
    done
    [ "$alive" -eq 0 ]
}

@test "local-mcp: a server behind sh's fork that ignores end of input and SIGTERM is still stopped, children too" {
    # the redirect makes sh fork the server instead of exec'ing it, so the
    # pid ask-claude started is sh's, not the server's
    FAKE_MCP_MODE=ignore-eof FAKE_MCP_PIDFILE="$TMP/pid" ASK_CLAUDE_CONFIRM=allow \
        run --separate-stderr replay --local-mcp "$SERVER 2>/dev/null" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.is_error == false'
    sleep 0.3
    gone "$(cat "$TMP/pid")" "$(cat "$TMP/pid.child")"
}

@test "local-mcp: tool inputs and results never reach a jq command line" {
    local real
    real=$(command -v jq)
    mkdir -p "$TMP/bin"
    printf '#!/bin/sh\nprintf "%%s\\n" "$*" >>"%s"\nexec "%s" "$@"\n' "$TMP/argv.log" "$real" >"$TMP/bin/jq"
    chmod +x "$TMP/bin/jq"
    PATH="$TMP/bin:$PATH" FAKE_MCP_SUFFIX=" RESULT-CANARY" ASK_CLAUDE_CONFIRM=allow \
        run --separate-stderr replay --local-mcp "$SERVER" "weather?" </dev/null
    [ "$status" -eq 0 ]
    result 2 | jq -e '.content[0].text == "Sunny in Paris RESULT-CANARY"'
    [ -s "$TMP/argv.log" ]
    if grep -q -e 'RESULT-CANARY' -e 'Paris' "$TMP/argv.log"; then
        grep -n -e 'RESULT-CANARY' -e 'Paris' "$TMP/argv.log" >&2
        return 1
    fi
}

@test "local-mcp: agent tools prints the server's tools as custom tool definitions" {
    run --separate-stderr env FAKE_MCP_TOOLS='["get_weather","get_time"]' "$BIN" agent tools --local-mcp "$SERVER"
    [ "$status" -eq 0 ]
    jq -e 'length == 2 and all(.[]; .type == "custom" and (.description | length > 0) and .input_schema.type == "object")' <<<"$output"
    jq -e 'map(.name) == ["get_weather", "get_time"]' <<<"$output"
}

@test "local-mcp: agent tools drops the \$schema key some servers put on every input schema" {
    run --separate-stderr env FAKE_MCP_SCHEMA_META=1 "$BIN" agent tools --local-mcp "$SERVER"
    [ "$status" -eq 0 ]
    jq -e 'all(.[]; .input_schema | has("$schema") | not)' <<<"$output"
    jq -e '.[0].input_schema.required == ["city"]' <<<"$output"
}

@test "local-mcp: agent tools needs no key, agent or environment, but needs a server" {
    run --separate-stderr env -u ANTHROPIC_API_KEY -u ASK_CLAUDE_AGENT -u ASK_CLAUDE_ENVIRONMENT "$BIN" agent tools --local-mcp "$SERVER"
    [ "$status" -eq 0 ]
    run --separate-stderr env -u ASK_CLAUDE_LOCAL_MCP "$BIN" agent tools
    [ "$status" -eq 2 ]
    grep -q -- '--local-mcp' <<<"$stderr"
}

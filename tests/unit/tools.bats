#!/usr/bin/env bats
# bats file_tags=rust-only
# shellcheck disable=SC2016  # jq programs in single quotes mention $ on purpose
load helpers

# The Messages tool loop (R8 to R10, R13, R14, R20): U11 of
# docs/plans/2026-11-06-0143-feat-rust-parity-and-local-tools-plan.md, KTD9,
# KTD11, KTD12. With --tools a turn runs every tool call the model makes, in
# order, behind the confirmation gate, and sends all the results back in one
# user message until the model stops calling; the chat on disk never ends in
# an unanswered tool_use. Also here: the gate scenarios that need the binary
# (piped stdin, two chat lines around a command reading stdin), since driving
# it needs --tools. The runner itself is rust/tests/tools.rs (U10).
#
# The streams under tests/fixtures/sse/tools/ are hand-written from text.sse,
# like the refusal and pause_turn ones: bash-call (one call), two-calls, and
# max-tokens (a call cut off mid-input). `turn` rewrites a call's input, so a
# test names the command it runs. They stand in for the live recording U11
# asks for, which also checks the no-beta-header assumption on the wire.
#
# What these tests pin beyond the plan's prose: the denial text is KTD11's;
# stderr lines are matched on what they name, not their wording.

# cd into a repository of its own, with HOME elsewhere: --tools is refused at
# $HOME (KTD10)
setup_repo() {
    mkdir -p "$TMP/repo" "$TMP/home"
    export HOME="$TMP/home"
    cd "$TMP/repo" || return
}

# cmd <command>: a bash tool input for it
cmd() { jq -n -c --arg c "$1" '{command: $c}'; }

# turn <n> <fixture> <input>...: replay turn <n> is tools/<fixture>.sse with
# block i's input replaced by the i-th <input>, whole, in one delta
turn() {
    local n="$1" fixture="$2" inputs
    shift 2
    inputs="$(printf '%s\n' "$@" | jq -R . | jq -s -c .)"
    mkdir -p "$TMP/replay"
    while IFS= read -r line; do
        case "$line" in
        "data: "*)
            printf 'data: %s\n' "$(jq -c --argjson in "$inputs" '
                if .type == "content_block_delta" and .delta.type == "input_json_delta" and .index < ($in | length)
                then .delta.partial_json = $in[.index] else . end' <<<"${line#data: }")"
            ;;
        *) printf '%s\n' "$line" ;;
        esac
    done <"$FIX/sse/tools/$fixture.sse" >"$TMP/replay/$n.sse"
}

# end_turn <n>: replay turn <n> ends the turn (text.sse)
end_turn() {
    mkdir -p "$TMP/replay"
    cp "$FIX/sse/text.sse" "$TMP/replay/$1.sse"
}

# tools <args...>: the binary with --tools over the replay directory
tools() {
    ASK_CLAUDE_REPLAY="$TMP/replay" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" --tools "$@"
}

request() { sed -n "$1p" "$TMP/requests.jsonl"; }
requests() { if [ -e "$TMP/requests.jsonl" ]; then wc -l <"$TMP/requests.jsonl" | tr -d ' '; else echo 0; fi; }
# results <n>: the tool_result blocks of request <n>'s last message
results() { request "$1" | jq -c '[.messages[-1].content[]? | select(.type == "tool_result")]'; }
# text_of: a tool_result's content as one string, string or text blocks
text_of() { jq -r '.content | if type == "string" then . else map(.text // "") | join("") end'; }
# chat_file: the first chat in the state directory, or nothing
chat_file() {
    local f
    for f in "$XDG_STATE_HOME"/ask-claude/*.json; do
        if [ -e "$f" ]; then printf '%s\n' "$f"; fi
        return 0
    done
}

# answered <file>: every tool_use in the messages array is answered by a
# tool_result in the message after it (R13)
answered() {
    jq -e '. as $m | [range(0; length) | select($m[.].role == "assistant")
        | . as $i | ($m[$i].content | if type == "array" then map(select(.type == "tool_use") | .id) else [] end)
        | select(length > 0) | . as $ids
        | ($m[$i + 1].content // [] | if type == "array" then map(select(.type == "tool_result") | .tool_use_id) else [] end) as $got
        | ($ids - $got) | length] | all(. == 0)' "$1"
}

# wait_for <path> [seconds]: until the path exists
wait_for() {
    local i
    for ((i = 0; i < ${2:-10} * 20; i++)); do
        [ -e "$1" ] && return 0
        sleep 0.05
    done
    echo "wait_for: $1 never appeared" >&2
    return 1
}

# exits_within <pid> <seconds>: the process ends in time; its status in $status
exits_within() {
    local i
    for ((i = 0; i < $2 * 20; i++)); do
        if ! kill -0 "$1" 2>/dev/null; then
            status=0
            wait "$1" || status=$?
            return 0
        fi
        sleep 0.05
    done
    kill -9 "$1" 2>/dev/null
    echo "exits_within: $1 still running after $2 s" >&2
    return 1
}

@test "tools: the request declares bash and the text editor only with --tools" {
    setup_repo
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "ping"
    [ "$status" -eq 0 ]
    request 1 | jq -e '[.tools[]?.type] | (index("bash_20250124") == null and index("text_editor_20250728") == null)'
    rm "$TMP/requests.jsonl"

    end_turn 1
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "ping" </dev/null
    [ "$status" -eq 0 ]
    request 1 | jq -e '.tools | any(.type == "bash_20250124" and .name == "bash")'
    request 1 | jq -e '.tools | any(.type == "text_editor_20250728" and .name == "str_replace_based_edit_tool")'
}

@test "tools: a bash call runs once and its result goes back as the first block of one user message" {
    setup_repo
    turn 1 bash-call "$(cmd 'echo pong | tee -a ran.log')"
    end_turn 2
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 2 ]
    [ "$(wc -l <ran.log)" -eq 1 ]
    request 2 | jq -e '.messages[-1].role == "user" and .messages[-1].content[0].type == "tool_result"'
    results 2 | jq -e 'length == 1 and .[0].tool_use_id == "toolu_FIXTURE01" and (.[0].is_error // false) == false'
    results 2 | jq -c '.[0]' | text_of | grep -qx 'pong'
    jq -s -e '.[0].tools == .[1].tools' "$TMP/requests.jsonl"
}

@test "tools: two calls in one reply give two results in one message, in order" {
    setup_repo
    turn 1 two-calls "$(cmd 'echo one')" "$(cmd 'echo two')"
    end_turn 2
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "go" </dev/null
    [ "$(requests)" -eq 2 ]
    results 2 | jq -e 'map(.tool_use_id) == ["toolu_FIXTURE01", "toolu_FIXTURE02"]'
    results 2 | jq -c '.[0]' | text_of | grep -qx 'one'
    results 2 | jq -c '.[1]' | text_of | grep -qx 'two'
}

@test "tools: under deny the result carries the denial and nothing runs" {
    setup_repo
    turn 1 bash-call "$(cmd 'touch ran')"
    end_turn 2
    ASK_CLAUDE_CONFIRM=deny run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 2 ]
    [ ! -e ran ]
    results 2 | jq -e '.[0].is_error == true'
    [ "$(results 2 | jq -c '.[0]' | text_of)" = "denied by the user; do not call this tool again for this request" ]
}

@test "tools: piped stdin with no policy denies with the existing message and runs nothing" {
    setup_repo
    turn 1 bash-call "$(cmd 'touch ran')"
    end_turn 2
    run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    [ ! -e ran ]
    results 2 | jq -e '.[0].is_error == true'
    grep -q 'stdin is not a terminal' <<<"$stderr"
}

@test "tools: --confirm deny wins over ASK_CLAUDE_CONFIRM=allow" {
    setup_repo
    turn 1 bash-call "$(cmd 'touch ran')"
    end_turn 2
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools --confirm deny "go" </dev/null
    [ "$status" -eq 0 ]
    [ ! -e ran ]
    results 2 | jq -e '.[0].is_error == true'
}

@test "tools: allow with --tools says on stderr that nothing will be asked" {
    setup_repo
    end_turn 1
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "ping" </dev/null
    [ "$status" -eq 0 ]
    grep -q 'ASK_CLAUDE_CONFIRM=allow' <<<"$stderr"
}

@test "tools: a command reading stdin does not eat the next chat line" {
    setup_repo
    turn 1 bash-call "$(cmd 'cat; echo after')"
    end_turn 2
    end_turn 3
    run --separate-stderr bash -c "printf 'first\nsecond\n/quit\n' | ASK_CLAUDE_CONFIRM=allow ASK_CLAUDE_REPLAY='$TMP/replay' ASK_CLAUDE_REQUEST_LOG='$TMP/requests.jsonl' '$BIN' --tools"
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 3 ]
    results 2 | jq -c '.[0]' | text_of | grep -qx 'after'
    request 3 | jq -e '.messages[-1] | .role == "user" and (.content == "second" or .content == [{type: "text", text: "second"}])'
}

@test "tools: max_tokens with a trailing tool call runs nothing and stores an error result" {
    setup_repo
    turn 1 max-tokens
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 1 ]
    [ ! -e ran ]
    chat="$(chat_file)"
    [ -n "$chat" ]
    jq -e '.[-1].role == "user" and (.[-1].content | any(.type == "tool_result" and .tool_use_id == "toolu_FIXTURE01" and .is_error == true))' "$chat"
    answered "$chat"
}

@test "tools: -s, --web or --mcp with --tools exits 2 before any request" {
    setup_repo
    end_turn 1
    # the control: --tools alone runs, so the exit 2 below is the combination's
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    rm "$TMP/requests.jsonl"
    for flags in "-s pptx" "--web" "--mcp https://mcp.example.com/mcp"; do
        # shellcheck disable=SC2086  # each set of flags is split on purpose
        ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools $flags "go" </dev/null
        [ "$status" -eq 2 ]
        [ ! -e "$TMP/requests.jsonl" ]
    done
}

@test "tools: ASK_CLAUDE_CONFIRM=yes with --tools exits 2; without --tools the one-shot runs" {
    setup_repo
    end_turn 1
    # the control: a valid policy runs, so the exit 2 below is the value's
    ASK_CLAUDE_CONFIRM=deny run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    rm "$TMP/requests.jsonl"
    ASK_CLAUDE_CONFIRM=yes run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 2 ]
    [ ! -e "$TMP/requests.jsonl" ]
    ASK_CLAUDE_CONFIRM=yes run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" "ping"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
}

@test "tools: ASK_CLAUDE_TOOLS turns tools on, 0 leaves them off, anything else exits 2" {
    setup_repo
    one() { ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" ASK_CLAUDE_CONFIRM=allow "$BIN" "ping" </dev/null; }
    ASK_CLAUDE_TOOLS=1 run --separate-stderr one
    [ "$status" -eq 0 ]
    request 1 | jq -e '.tools | any(.type == "bash_20250124")'
    rm "$TMP/requests.jsonl"
    ASK_CLAUDE_TOOLS=0 run --separate-stderr one
    [ "$status" -eq 0 ]
    request 1 | jq -e '[.tools[]?.type] | index("bash_20250124") == null'
    rm "$TMP/requests.jsonl"
    ASK_CLAUDE_TOOLS=maybe run --separate-stderr one
    [ "$status" -eq 2 ]
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "tools: nine tool rounds complete, past the eight pause_turn continuations" {
    setup_repo
    turn 1 bash-call "$(cmd 'echo round')"
    for n in 2 3 4 5 6 7 8 9; do cp "$TMP/replay/1.sse" "$TMP/replay/$n.sse"; done
    end_turn 10
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 10 ]
    request 10 | jq -e '.messages | length == 19'
}

@test "tools: a model that keeps calling past the round cap ends with error results" {
    setup_repo
    turn 1 bash-call "$(cmd 'echo again')"
    for ((n = 2; n <= 70; n++)); do cp "$TMP/replay/1.sse" "$TMP/replay/$n.sse"; done
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    [ "$(requests)" -ge 64 ]
    [ "$(requests)" -le 65 ]
    chat="$(chat_file)"
    jq -e '.[-1].role == "user" and (.[-1].content | any(.type == "tool_result" and .is_error == true))' "$chat"
    answered "$chat"
}

@test "tools: /history and resume render a tool turn through safe" {
    setup_repo
    esc="$(printf '\033')"
    turn 1 bash-call "$(cmd "echo HIST-MARKER${esc}]0;retitled$(printf '\007')")"
    end_turn 2
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr tools "go" </dev/null
    [ "$status" -eq 0 ]
    name="$(basename "$(chat_file)" .json)"
    run --separate-stderr bash -c "printf '/history\n/quit\n' | ASK_CLAUDE_REPLAY='$FIX/sse/text.sse' '$BIN' --tools resume '$name'"
    [ "$status" -eq 0 ]
    grep -q 'HIST-MARKER' <<<"$stderr"
    grep -q 'HIST-MARKER' <<<"$output"
    refute grep -qF "$esc" <<<"$stderr$output"
}

@test "tools: after Ctrl-C mid-command the chat holds an error result and resume sends a whole body" {
    setup_repo
    turn 1 bash-call "$(cmd 'touch started; sleep 30')"
    end_turn 2
    ASK_CLAUDE_CONFIRM=allow ASK_CLAUDE_REPLAY="$TMP/replay" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        "$BIN" --tools "go" </dev/null >"$TMP/out" 2>"$TMP/err" 3>&- 7>&- &
    pid=$!
    wait_for started
    kill -INT "$pid"
    exits_within "$pid" 10
    [ "$status" -eq 130 ]
    chat="$(chat_file)"
    jq -e '.[-1].role == "user" and (.[-1].content | any(.type == "tool_result" and .tool_use_id == "toolu_FIXTURE01" and .is_error == true))' "$chat"
    answered "$chat"

    rm "$TMP/requests.jsonl"
    name="$(basename "$chat" .json)"
    run --separate-stderr bash -c "printf 'again\n/quit\n' | ASK_CLAUDE_CONFIRM=allow ASK_CLAUDE_REPLAY='$FIX/sse/text.sse' ASK_CLAUDE_REQUEST_LOG='$TMP/requests.jsonl' '$BIN' --tools resume '$name'"
    [ "$status" -eq 0 ]
    request 1 | jq '.messages' >"$TMP/resumed.json"
    answered "$TMP/resumed.json"
}

@test "tools: Ctrl-C while the reply stream blocks exits 130 within two seconds, the chat whole" {
    setup_repo
    turn 1 bash-call "$(cmd 'echo one')"
    mkfifo "$TMP/replay/2.sse"
    # held open read-write on fd 7 (bats owns fd 3), so the binary's open does
    # not block and its read does
    exec 7<>"$TMP/replay/2.sse"
    sed -n '1,3p' "$FIX/sse/text.sse" >&7
    ASK_CLAUDE_CONFIRM=allow ASK_CLAUDE_REPLAY="$TMP/replay" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        "$BIN" --tools "go" </dev/null >"$TMP/out" 2>"$TMP/err" 3>&- 7>&- &
    pid=$!
    for ((i = 0; i < 200 && $(requests) < 2; i++)); do sleep 0.05; done
    [ "$(requests)" -eq 2 ]
    sleep 0.2
    kill -INT "$pid"
    exits_within "$pid" 2
    exec 7>&-
    [ "$status" -eq 130 ]
    chat="$(chat_file)"
    if [ -n "$chat" ]; then answered "$chat"; fi
}

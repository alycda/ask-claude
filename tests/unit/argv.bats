#!/usr/bin/env bats
# bats file_tags=bash-only
# What another local account can read: a process's command line. Neither the
# key nor request text (a -f file, a typed line) may appear on the command
# line of anything the script starts. A jq shim first on PATH records every
# jq argv.

load helpers

export ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE

# absent <text>: fail when the recorded argv contains <text>. A plain
# `! grep` would not fail a bats test unless it were the last line.
absent() {
    if grep -q -- "$1" "$TMP/argv.log"; then
        echo "on jq's command line: $1" >&2
        return 1
    fi
}

setup_shim() {
    local real
    real=$(command -v jq)
    mkdir -p "$TMP/bin"
    printf '#!/bin/sh\nprintf "%%s\\n" "$*" >>"%s"\nexec "%s" "$@"\n' "$TMP/argv.log" "$real" >"$TMP/bin/jq"
    chmod +x "$TMP/bin/jq"
    export PATH="$TMP/bin:$PATH"
}

@test "argv: a -f file and chat lines never reach jq's command line" {
    setup_shim
    printf 'FILE-CANARY\n' >"$TMP/notes.txt"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" -f "$TMP/notes.txt" "summarise"
    [ "$status" -eq 0 ]
    printf 'CHAT-CANARY\n/quit\n' | ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" >/dev/null 2>&1
    [ -s "$TMP/argv.log" ] # the shim saw jq run
    absent 'FILE-CANARY'
    absent 'CHAT-CANARY'
    absent 'sk-ant-test-not-a-key'
    # the text still reached the history
    grep -q 'FILE-CANARY' "$XDG_STATE_HOME"/ask-claude/*.json
}

@test "argv: agent chat lines never reach jq's command line" {
    setup_shim
    local usage='data: {"type":"session.usage","usage":{"input_tokens":1,"output_tokens":1}}'
    mkdir -p "$TMP/fx"
    printf '%s\n\n' "$usage" 'data: {"type":"session.status_idle","stop_reason":{"type":"end_turn"}}' | tee "$TMP/fx/1.sse" >"$TMP/fx/2.sse"
    printf 'AGENT-FIRST-CANARY\nAGENT-NEXT-CANARY\n/quit\n' |
        ASK_CLAUDE_REPLAY="$TMP/fx" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent >/dev/null 2>&1
    [ -s "$TMP/argv.log" ]
    absent 'AGENT-FIRST-CANARY'
    absent 'AGENT-NEXT-CANARY'
    # and both still went out
    grep -q 'AGENT-FIRST-CANARY' "$TMP/requests.jsonl"
    grep -q 'AGENT-NEXT-CANARY' "$TMP/requests.jsonl"
}

#!/usr/bin/env bats
load helpers

# -f FILE with agent uploads the file (Files API) and attaches it to the
# session; `agent files <sesn> [name]...` lists what the session holds and
# downloads the named ones. Replay: the upload is logged as {"file": name}
# and answers file_FIXTURE01; files.json is the list page; files/<name> is a
# download.

export ASK_CLAUDE_AGENT=agent_FIXTURE ASK_CLAUDE_ENVIRONMENT=env_FIXTURE
replay() { ASK_CLAUDE_REPLAY="$FIX/sessions/files" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" agent "$@"; }
request() { sed -n "$1p" "$TMP/requests.jsonl"; }

@test "files: -f uploads the file and attaches it; the prompt is not prefixed" {
    printf 'line one\n' > "$TMP/notes.md"
    run --separate-stderr replay -f "$TMP/notes.md" "summarize"
    [ "$status" -eq 0 ]
    [ "$output" = "pong" ]
    [ "$(wc -l < "$TMP/requests.jsonl")" -eq 2 ]
    request 1 | jq -e '.file=="notes.md"'
    request 2 | jq -S . > "$TMP/got.json"
    jq -S . "$FIX/agents/create-file.json" > "$TMP/want.json"
    assert_golden "$TMP/want.json" "$TMP/got.json"
}

@test "files: an unreadable -f exits 2 before any request" {
    run --separate-stderr replay -f "$TMP/missing.md" "summarize"
    [ "$status" -eq 2 ]
    grep -q 'cannot read' <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "files: list prints name, size and id" {
    run --separate-stderr replay files sesn_FIXTURE01
    [ "$status" -eq 0 ]
    grep -qE '^notes.txt\s+17\s+file_FIXTURE01$' <<<"$output"
    grep -qE '^result.txt\s+4\s+file_FIXTURE02$' <<<"$output"
}

@test "files: downloads land in the current directory, mode 600" {
    cd "$TMP"
    run --separate-stderr replay files sesn_FIXTURE01 result.txt
    [ "$status" -eq 0 ]
    [ "$(cat "$TMP/result.txt")" = "OUT" ]
    [ "$(stat -c %a "$TMP/result.txt" 2>/dev/null || stat -f %Lp "$TMP/result.txt")" = "600" ]
    grep -q 'result.txt' <<<"$stderr"
}

@test "files: an existing file is not overwritten without --force" {
    cd "$TMP"
    printf 'mine\n' > "$TMP/result.txt"
    run --separate-stderr replay files sesn_FIXTURE01 result.txt
    [ "$status" -eq 2 ]
    grep -q -- '--force' <<<"$stderr"
    [ "$(cat "$TMP/result.txt")" = "mine" ]
    run --separate-stderr replay --force files sesn_FIXTURE01 result.txt
    [ "$status" -eq 0 ]
    [ "$(cat "$TMP/result.txt")" = "OUT" ]
}

@test "files: a name the session does not have exits 2, and a path is refused" {
    cd "$TMP"
    run --separate-stderr replay files sesn_FIXTURE01 nope.txt
    [ "$status" -eq 2 ]
    grep -q 'nope.txt' <<<"$stderr"
    run --separate-stderr replay files sesn_FIXTURE01 ../result.txt
    [ "$status" -eq 2 ]
}

@test "files: a prefix of a session started here works, and a bad list page exits 1" {
    mkdir -p "$XDG_STATE_HOME/ask-claude/sessions"
    printf 'agent=a\n' > "$XDG_STATE_HOME/ask-claude/sessions/sesn_FIXTURE01"
    run --separate-stderr replay files FIXTURE01
    [ "$status" -eq 0 ]
    grep -q 'result.txt' <<<"$output"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sessions/not-sse" "$BIN" agent files sesn_FIXTURE01
    [ "$status" -eq 1 ]
    grep -q 'invalid x-api-key' <<<"$stderr"
}

@test "files: a planted <name>.part symlink does not redirect the download" {
    cd "$TMP"
    printf 'precious\n' >"$TMP/victim.txt"
    ln -s "$TMP/victim.txt" "$TMP/result.txt.part"
    run --separate-stderr replay files sesn_FIXTURE01 result.txt
    [ "$status" -eq 0 ]
    [ "$(cat "$TMP/victim.txt")" = precious ]
    [ -f "$TMP/result.txt" ] && [ ! -L "$TMP/result.txt" ]
    [ "$(cat "$TMP/result.txt")" = "OUT" ]
}

@test "files: --force never replaces a symlink or a directory" {
    cd "$TMP"
    printf 'precious\n' >"$TMP/victim.txt"
    ln -s "$TMP/victim.txt" "$TMP/result.txt"
    run --separate-stderr replay --force files sesn_FIXTURE01 result.txt
    [ "$status" -eq 2 ]
    [ "$(cat "$TMP/victim.txt")" = precious ]
    unlink "$TMP/result.txt"
    mkdir "$TMP/result.txt"
    run --separate-stderr replay --force files sesn_FIXTURE01 result.txt
    [ "$status" -eq 2 ]
    [ -z "$(ls -A "$TMP/result.txt")" ]
}

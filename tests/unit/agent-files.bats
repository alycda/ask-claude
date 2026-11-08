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

# no_column: a `column` first on PATH that fails the way a missing one does.
# Debian and Ubuntu leave column (bsdextrautils) out of minimal images, so
# the script aligns the list itself.
no_column() {
    mkdir -p "$TMP/bin"
    printf '#!/bin/sh\necho "column: command not found" >&2\nexit 127\n' >"$TMP/bin/column"
    chmod +x "$TMP/bin/column"
    export PATH="$TMP/bin:$PATH"
}

# page <jq expr>: a replay directory whose files list is <jq expr>
page() {
    mkdir -p "$TMP/page"
    jq -n "$1" >"$TMP/page/files.json"
}

@test "files: the list is aligned without column" {
    no_column
    run --separate-stderr replay files sesn_FIXTURE01
    [ "$status" -eq 0 ]
    [ -z "$stderr" ]
    [ "$output" = "$(printf '%s\n' 'notes.txt   17  file_FIXTURE01' 'result.txt  4   file_FIXTURE02')" ]
}

@test "files: an empty list prints nothing and exits 0" {
    no_column
    page '{data: []}'
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/page" "$BIN" agent files sesn_FIXTURE01
    [ "$status" -eq 0 ]
    [ -z "$output" ]
    [ -z "$stderr" ]
}

@test "files: a name with a tab, a newline or an escape stays on its own line" {
    no_column
    page '{data: [{id: "file_A", filename: "a\tb\nc\u001b[2Jd", size_bytes: 1},
                  {id: "file_B", filename: "e", size_bytes: 22}]}'
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/page" "$BIN" agent files sesn_FIXTURE01
    [ "$status" -eq 0 ]
    [ "$output" = "$(printf '%s\n' 'a\tb\nc[2Jd  1   file_A' 'e            22  file_B')" ]
}

@test "files: a list page the script cannot print exits 1, not 0 with nothing" {
    page '{data: [{id: "file_A", filename: {"not": "a name"}, size_bytes: 1}]}'
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/page" "$BIN" agent files sesn_FIXTURE01
    [ "$status" -eq 1 ]
    [ -z "$output" ]
    grep -q 'file list' <<<"$stderr"
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

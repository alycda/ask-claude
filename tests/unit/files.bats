#!/usr/bin/env bats
load helpers

# `ask-claude files <chat> [name]...`: the files code execution wrote in a
# Chat, read back through the Files API. A bash_code_execution result lists
# what the command left in $OUTPUT_DIR as {"type":
# "bash_code_execution_output", "file_id": ...} entries; the Chat keeps the
# assistant content, so the ids are on disk and only the names, sizes and
# bytes need the API (GET /v1/files/<id>, GET /v1/files/<id>/content).
#
# Replay: a one-shot over sse/code-execution-file.sse makes the Chat; then
# ASK_CLAUDE_REPLAY is a directory holding <id>.json (the metadata page) and
# <id> (the content) for each file, tests/fixtures/files by default.

STATE_DIR() { echo "$XDG_STATE_HOME/ask-claude"; }

# chat_with <sse>: a one-shot with -s over <sse>; CHAT is the Chat's name
chat_with() {
    ASK_CLAUDE_REPLAY="$1" "$BIN" -s pptx "run" >/dev/null 2>&1
    CHAT=$(basename "$(ls "$(STATE_DIR)"/*.json)" .json)
}

# again <sse>: one more turn of CHAT over <sse>, through the chat loop
again() {
    bash -c "printf 'again\n/quit\n' | ASK_CLAUDE_REPLAY='$1' '$BIN' -s pptx resume $CHAT" >/dev/null 2>&1
}

# with_id <id>: code-execution-file.sse whose file entry carries <id>
with_id() {
    sed "s|file_FIXTURE01|$1|" "$FIX/sse/code-execution-file.sse" >"$TMP/$2.sse"
    grep -qF "\"file_id\":\"$1\"" "$TMP/$2.sse" || { echo "with_id: $1 not injected"; return 1; }
}

# pages: a replay directory with the fixture pages, for a test to add to
pages() {
    mkdir -p "$TMP/pages"
    cp "$FIX/files/"* "$TMP/pages/"
}

files() { ASK_CLAUDE_REPLAY="${PAGES:-$FIX/files}" "$BIN" files "$@"; }

# refused: exit 2 from the binary, not from the parser, which exits 2 with
# a usage on any argument it does not know (today's binary on `files`)
refused() {
    [ "$status" -eq 2 ]
    refute grep -q '^Usage:' <<<"$stderr"
}

@test "files: the reply that wrote a file prints as before and names the command" {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/code-execution-file.sse" "$BIN" -s pptx "run"
    [ "$status" -eq 0 ]
    printf '%s' "$output" >"$TMP/got.txt"
    printf '%s' "$(cat "$FIX/sse/code-execution-file.stream.txt")" >"$TMP/want.txt"
    assert_golden "$TMP/want.txt" "$TMP/got.txt"
    name=$(basename "$(ls "$(STATE_DIR)"/*.json)" .json)
    grep -qFx "[1 file: ask-claude files $name]" <<<"$stderr"
}

@test "files: a reply that wrote no file names no command" {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/code-execution.sse" "$BIN" -s pptx "run"
    [ "$status" -eq 0 ]
    refute grep -qF 'ask-claude files' <<<"$stderr"
}

@test "files: list prints name, size and id" {
    chat_with "$FIX/sse/code-execution-file.sse"
    run --separate-stderr files "$CHAT"
    [ "$status" -eq 0 ]
    [ "$output" = 'report.md  9  file_FIXTURE01' ]
    [ -z "$stderr" ]
}

@test "files: a prefix of the chat works" {
    chat_with "$FIX/sse/code-execution-file.sse"
    run --separate-stderr files "${CHAT:0:8}"
    [ "$status" -eq 0 ]
    [ "$output" = 'report.md  9  file_FIXTURE01' ]
}

@test "files: an unknown chat exits 2 before any request" {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/none" "$BIN" files nope
    refused
    grep -q 'nope' <<<"$stderr"
}

@test "files: a chat with no file prints nothing, exits 0 and asks the API nothing" {
    chat_with "$FIX/sse/code-execution.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/none" "$BIN" files "$CHAT"
    [ "$status" -eq 0 ]
    [ -z "$output" ]
    [ -z "$stderr" ]
}

@test "files: each id once, in the order the chat first wrote it" {
    pages
    jq '.id="file_FIXTURE02" | .filename="chart.png" | .size_bytes=3' \
        "$FIX/files/file_FIXTURE01.json" >"$TMP/pages/file_FIXTURE02.json"
    printf 'PNG' >"$TMP/pages/file_FIXTURE02"
    with_id file_FIXTURE02 second
    chat_with "$FIX/sse/code-execution-file.sse"
    again "$TMP/second.sse"
    again "$FIX/sse/code-execution-file.sse"
    PAGES="$TMP/pages" run --separate-stderr files "$CHAT"
    [ "$status" -eq 0 ]
    [ "$output" = "$(printf '%s\n' 'report.md  9  file_FIXTURE01' 'chart.png  3  file_FIXTURE02')" ]
}

@test "files: an id the API cannot find is said on stderr, the rest listed, exit 1" {
    pages
    jq -n '{type: "error", error: {type: "not_found_error", message: "File not found"}}' \
        >"$TMP/pages/file_GONE.json"
    with_id file_GONE gone
    chat_with "$TMP/gone.sse"
    again "$FIX/sse/code-execution-file.sse"
    PAGES="$TMP/pages" run --separate-stderr files "$CHAT"
    [ "$status" -eq 1 ]
    [ "$output" = 'report.md  9  file_FIXTURE01' ]
    grep -q 'file_GONE' <<<"$stderr"
    grep -q 'File not found' <<<"$stderr"
}

@test "files: an id that is not a file id is refused, never sent" {
    with_id '../../sessions' bad
    chat_with "$TMP/bad.sse"
    run --separate-stderr env ASK_CLAUDE_REPLAY="$TMP/none" "$BIN" files "$CHAT"
    [ "$status" -eq 1 ]
    [ -z "$output" ]
    grep -qF '../../sessions' <<<"$stderr"
}

@test "files: a name with a tab, a newline or an escape stays on its own line" {
    pages
    jq '.filename="a\tb\nc\u001b[2Jd"' "$FIX/files/file_FIXTURE01.json" >"$TMP/pages/file_FIXTURE01.json"
    chat_with "$FIX/sse/code-execution-file.sse"
    PAGES="$TMP/pages" run --separate-stderr files "$CHAT"
    [ "$status" -eq 0 ]
    [ "$output" = 'a\tb\nc[2Jd  9  file_FIXTURE01' ]
}

@test "files: without a key the list exits 2 and says which variable" {
    chat_with "$FIX/sse/code-execution-file.sse"
    run --separate-stderr env -u ANTHROPIC_API_KEY ASK_CLAUDE_REPLAY="$FIX/files" "$BIN" files "$CHAT"
    refused
    grep -q 'ANTHROPIC_API_KEY' <<<"$stderr"
}

@test "files: downloads land in the current directory, mode 600" {
    chat_with "$FIX/sse/code-execution-file.sse"
    cd "$TMP"
    run --separate-stderr files "$CHAT" report.md
    [ "$status" -eq 0 ]
    cmp "$FIX/files/file_FIXTURE01" "$TMP/report.md"
    [ "$(stat -c %a "$TMP/report.md" 2>/dev/null || stat -f %Lp "$TMP/report.md")" = "600" ]
    grep -q 'report.md' <<<"$stderr"
}

@test "files: an existing file is not overwritten without --force" {
    chat_with "$FIX/sse/code-execution-file.sse"
    cd "$TMP"
    printf 'mine\n' >"$TMP/report.md"
    run --separate-stderr files "$CHAT" report.md
    refused
    grep -q -- '--force' <<<"$stderr"
    [ "$(cat "$TMP/report.md")" = "mine" ]
    run --separate-stderr files --force "$CHAT" report.md
    [ "$status" -eq 0 ]
    cmp "$FIX/files/file_FIXTURE01" "$TMP/report.md"
}

@test "files: a name the chat does not have exits 2, and a path is refused" {
    chat_with "$FIX/sse/code-execution-file.sse"
    cd "$TMP"
    run --separate-stderr files "$CHAT" nope.md
    refused
    grep -q 'nope.md' <<<"$stderr"
    run --separate-stderr files "$CHAT" ../report.md
    refused
    [ ! -e "$TMP/../report.md" ]
}

# The filename is the API's, and so the model's: a name that is a path, or
# a dotfile, is listed (through safe) but never written.
@test "files: a listed name that is a path or a dotfile is never written" {
    pages
    chat_with "$FIX/sse/code-execution-file.sse"
    mkdir -p "$TMP/cwd"
    cd "$TMP/cwd"
    for bad in '../escape' '.profile'; do
        jq --arg n "$bad" '.filename=$n' "$FIX/files/file_FIXTURE01.json" >"$TMP/pages/file_FIXTURE01.json"
        PAGES="$TMP/pages" run --separate-stderr files "$CHAT"
        [ "$status" -eq 0 ]
        grep -qF "$bad" <<<"$output"
        PAGES="$TMP/pages" run --separate-stderr files "$CHAT" "$bad"
        refused
    done
    [ ! -e "$TMP/escape" ]
    [ -z "$(ls -A "$TMP/cwd")" ]
}

@test "files: a planted <name>.part symlink does not redirect the download" {
    chat_with "$FIX/sse/code-execution-file.sse"
    cd "$TMP"
    printf 'precious\n' >"$TMP/victim.txt"
    ln -s "$TMP/victim.txt" "$TMP/report.md.part"
    run --separate-stderr files "$CHAT" report.md
    [ "$status" -eq 0 ]
    [ "$(cat "$TMP/victim.txt")" = precious ]
    [ -f "$TMP/report.md" ]
    [ ! -L "$TMP/report.md" ]
    cmp "$FIX/files/file_FIXTURE01" "$TMP/report.md"
}

@test "files: --force never replaces a symlink or a directory" {
    chat_with "$FIX/sse/code-execution-file.sse"
    cd "$TMP"
    printf 'precious\n' >"$TMP/victim.txt"
    ln -s "$TMP/victim.txt" "$TMP/report.md"
    run --separate-stderr files --force "$CHAT" report.md
    refused
    [ "$(cat "$TMP/victim.txt")" = precious ]
    unlink "$TMP/report.md"
    mkdir "$TMP/report.md"
    run --separate-stderr files --force "$CHAT" report.md
    refused
    [ -z "$(ls -A "$TMP/report.md")" ]
}

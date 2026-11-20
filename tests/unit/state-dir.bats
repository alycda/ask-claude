#!/usr/bin/env bats
# The state directory holds the history and fixed names (.always.<pid>,
# .stream.<pid>, ...); an .always.<pid> file allows every tool confirmation
# of that process. The script refuses a $STATE another account could have
# planted those names in: a symlink, one it cannot create or make private,
# one another account owns, one others can write. Nothing is read or sent
# before the refusal.

load helpers

# shim <name> <script body>: a command first on PATH
shim() {
    mkdir -p "$TMP/bin"
    printf '#!/bin/sh\n%s\n' "$2" >"$TMP/bin/$1"
    chmod +x "$TMP/bin/$1"
    export PATH="$TMP/bin:$PATH"
}

# refused <pattern>: a one shot over a recorded reply exits 2 naming the
# problem, and no request was made
refused() {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" hi
    [ "$status" -eq 2 ]
    grep -q -- "$1" <<<"$stderr"
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "state-dir: a private directory of our own is used" {
    run --separate-stderr env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" hi
    [ "$status" -eq 0 ]
    [ "$(stat -c %a "$XDG_STATE_HOME/ask-claude" 2>/dev/null || stat -f %Lp "$XDG_STATE_HOME/ask-claude")" = "700" ]
}

@test "state-dir: a symlink is refused, even to a private directory of our own" {
    mkdir -p "$TMP/elsewhere" "$XDG_STATE_HOME"
    chmod 700 "$TMP/elsewhere"
    ln -s "$TMP/elsewhere" "$XDG_STATE_HOME/ask-claude"
    refused 'is a symlink'
    [ -z "$(ls -A "$TMP/elsewhere")" ]
}

@test "state-dir: a directory that cannot be made private is refused" {
    shim chmod 'echo "chmod: Operation not permitted" >&2; exit 1'
    refused 'cannot make .* private'
}

@test "state-dir: a directory that cannot be created is refused" {
    mkdir -p "$TMP/state"
    : >"$XDG_STATE_HOME/ask-claude" # a file where the directory goes
    refused 'cannot create'
}

@test "state-dir: a directory another account owns is refused" {
    # the script compares the directory's owner with id -u; a shim reports
    # another uid, so the check runs without a second account
    # shellcheck disable=SC2016  # the shim's own $1 and $@, expanded when it runs
    shim id 'if [ "$1" = -u ]; then echo 99999; else exec /usr/bin/id "$@"; fi'
    refused 'owned by uid'
}

@test "state-dir: a directory another account owns is refused (real chown, root only)" {
    [ "$(id -u)" -eq 0 ] || skip "needs root to make a directory another account owns"
    mkdir -p "$XDG_STATE_HOME/ask-claude"
    chmod 755 "$XDG_STATE_HOME/ask-claude"
    chown 65534 "$XDG_STATE_HOME/ask-claude"
    # as root, chmod 700 succeeds on any directory; only the owner check stops it
    refused 'owned by uid 65534'
}

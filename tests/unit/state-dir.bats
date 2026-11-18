#!/usr/bin/env bats
# The state directory holds the history and fixed names (.always.<pid>,
# .stream.<pid>, ...); an .always.<pid> file allows every tool confirmation
# of that process. The script refuses a $STATE another account could have
# planted those names in: a symlink, one it cannot create or make private,
# one another account owns, one others can write. Nothing is read or sent
# before the refusal.

load helpers

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

@test "state-dir: a directory that cannot be created is refused" {
    mkdir -p "$TMP/state"
    : >"$XDG_STATE_HOME/ask-claude" # a file where the directory goes
    refused 'cannot create'
}

# `${XDG_STATE_HOME:-$HOME/.local/state}` under `set -u`: with neither set
# the script stops at the unbound HOME; it never falls back to a relative
# .local/state under the current directory.
@test "state-dir: no XDG_STATE_HOME and no HOME is refused, nothing made under the cwd" {
    mkdir -p "$TMP/cwd"
    cd "$TMP/cwd"
    run --separate-stderr env -u XDG_STATE_HOME -u HOME ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" hi
    [ "$status" -ne 0 ]
    grep -q 'HOME: unbound variable' <<<"$stderr"
    [ -z "$(ls -A "$TMP/cwd")" ]
    [ ! -e "$TMP/requests.jsonl" ]
    # an empty HOME names the root, which only root can write
    [ "$(id -u)" -ne 0 ] || return 0
    run --separate-stderr env -u XDG_STATE_HOME HOME= ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" hi
    [ "$status" -eq 2 ]
    grep -q 'cannot create /.local/state/ask-claude' <<<"$stderr"
    [ -z "$(ls -A "$TMP/cwd")" ]
    [ ! -e "$TMP/requests.jsonl" ]
}

@test "state-dir: a directory another account owns is refused (real chown, root only)" {
    [ "$(id -u)" -eq 0 ] || skip "needs root to make a directory another account owns"
    mkdir -p "$XDG_STATE_HOME/ask-claude"
    chmod 755 "$XDG_STATE_HOME/ask-claude"
    chown 65534 "$XDG_STATE_HOME/ask-claude"
    # as root, chmod 700 succeeds on any directory; only the owner check stops it
    refused 'owned by uid 65534'
}

# A directory that cannot be made private, staged through the file system: a
# directory root owns, which another account cannot chmod.
@test "state-dir: a directory whose mode cannot be set is refused (another account, root only)" {
    [ "$(id -u)" -eq 0 ] || skip "needs root to run the binary as an account that does not own the directory"
    command -v setpriv >/dev/null || skip "needs setpriv (util-linux) to drop to another account"
    mkdir -p "$XDG_STATE_HOME/ask-claude"
    chmod 755 "$TMP" "$XDG_STATE_HOME" "$XDG_STATE_HOME/ask-claude"
    run --separate-stderr setpriv --reuid=65534 --regid=65534 --clear-groups \
        env ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN" hi
    [ "$status" -eq 2 ]
    grep -q 'cannot make .* private' <<<"$stderr"
    [ "$(stat -c %a "$XDG_STATE_HOME/ask-claude" 2>/dev/null || stat -f %Lp "$XDG_STATE_HOME/ask-claude")" = "755" ]
}

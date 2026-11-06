# Shared setup for the bats suite. Loaded with `load helpers` from each file.
#
# ROOT     repo root
# BIN      the binary under test; ASK_CLAUDE_BIN overrides it so the same
#          suite can run against the Rust port (plan R5)
# FIX      tests/fixtures
# TMP      per-test scratch dir, removed in teardown; XDG_STATE_HOME points
#          inside it so no test touches ~/.local/state

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
BIN="${ASK_CLAUDE_BIN:-$ROOT/ask-claude}"
FIX="$ROOT/tests/fixtures"

setup() {
    TMP="$(mktemp -d)"
    export XDG_STATE_HOME="$TMP/state"
    export ANTHROPIC_API_KEY="sk-ant-test-not-a-key"
}

teardown() {
    rm -rf "$TMP"
}

# events <fixture>: the JSON payloads of an SSE fixture, one per line,
# exactly as claude.sh's `read` loop stripped them
events() {
    sed -n 's/^data: //p' "$FIX/sse/$1.sse"
}

# assert_golden <expected-file> <actual-file>: byte-equal, diff on failure
assert_golden() {
    if ! diff -u "$1" "$2"; then
        echo "golden mismatch: $1" >&2
        return 1
    fi
}

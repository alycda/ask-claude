#!/usr/bin/env bats
load helpers

# The script sources its bash modules from lib/, next to the jq programs.
# An install that lost one must stop at once with the file's name, exit 2,
# not run on and fail later with "command not found" (the script runs
# without -e). Each test runs a copy of the script and lib/ from $TMP, so
# the checkout is never touched.

LIBS=(config common chat messages local-mcp agent-api agent-turn)

# install [skip]: copy ask-claude and lib/ to $TMP/install, without
# lib/<skip>.bash when given
install() {
    mkdir -p "$TMP/install"
    cp "$ROOT/ask-claude" "$TMP/install/"
    cp -R "$ROOT/lib" "$TMP/install/"
    [ -z "${1:-}" ] || rm -f "$TMP/install/lib/$1.bash"
}

# bats test_tags=bash-only
@test "layout: a whole copy of the script and lib/ runs from anywhere" {
    install
    local m
    for m in "${LIBS[@]}"; do
        [ -f "$TMP/install/lib/$m.bash" ]
    done
    cd "$TMP"
    run --separate-stderr "$TMP/install/ask-claude" list
    [ "$status" -eq 0 ]
}

# bats test_tags=bash-only
@test "layout: a missing lib file is exit 2 and names the file" {
    local m
    for m in "${LIBS[@]}"; do
        rm -rf "$TMP/install"
        install "$m"
        run --separate-stderr "$TMP/install/ask-claude" list
        [ "$status" -eq 2 ]
        grep -q "^ask-claude: missing .*/lib/$m\.bash" <<<"$stderr"
        refute grep -q 'command not found' <<<"$stderr"
    done
}

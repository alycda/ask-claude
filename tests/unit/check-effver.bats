#!/usr/bin/env bats
# tools/effver/check-effver over a throwaway git repo: commit contents are
# data, never code (docs/security-audit, check-effver finding)

load helpers

CHECK="$ROOT/tools/effver/check-effver"

# commit <version> <effort>: a commit with VERSION and a matching changelog
commit() {
    printf '%s\n' "$1" >VERSION
    printf '# Changelog\n\n## %s (%s) - 2026-11-05\n\n- a change\n' "$1" "$2" >CHANGELOG.md
    git add -A
    git -c user.name=t -c user.email=t@t commit -q -m "v $1"
}

setup_repo() {
    cd "$TMP" || return
    git init -q -b main repo
    cd repo || return
}

@test "check-effver: a well-formed micro bump passes" {
    setup_repo
    commit 0.4.0 macro
    commit 0.4.1 micro
    run "$CHECK" main~1..main
    [ "$status" -eq 0 ]
}

@test "check-effver: a parent VERSION is data, never evaluated as arithmetic" {
    setup_repo
    commit 0.4.0 macro
    # shellcheck disable=SC2016  # the payload must reach VERSION literally
    printf '%s\n' '0.4.x[$(touch '"$TMP"'/MARKER)]' >VERSION
    git add -A
    git -c user.name=t -c user.email=t@t commit -q -m "crafted parent"
    commit 0.4.1 micro
    run "$CHECK" main~2..main
    [ "$status" -eq 1 ]
    [ ! -e "$TMP/MARKER" ]
    [[ $output == *"not MACRO.MESO.MICRO"* ]]
}

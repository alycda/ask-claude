#!/usr/bin/env bats
# `just effort-floor` and `just check-effver` in git mode: a throwaway git
# repo with no .jj, the EffVer recipes imported from the tree under test, and
# a minimal ask-claude whose #USAGE block is the spec usage diffs.

load helpers

# spec <flag>...: the script, one #USAGE flag line per argument
spec() {
    {
        printf '#!/usr/bin/env -S usage bash\n'
        for f in "$@"; do printf '#USAGE flag "%s"\n' "$f"; done
        printf 'echo hi\n'
    } >ask-claude
}

commit() {
    git add -A
    git -c user.name=t -c user.email=t@t commit -q -m "$1"
}

# release <version> <effort>: VERSION and a matching CHANGELOG heading, committed
release() {
    printf '%s\n' "$1" >VERSION
    printf '# Changelog\n\n## %s (%s) - 2026-11-06\n\n- a change\n' "$1" "$2" >CHANGELOG.md
    commit "version($1): $2"
}

# tools: the recipes and scripts from the tree under test, and a justfile
tools() {
    mkdir -p tools/effver
    cp "$ROOT/tools/effver/effver.just" "$ROOT/tools/effver/check-effver" "$ROOT/tools/effver/bump-effver" tools/effver/
    printf "import 'tools/effver/effver.just'\n" >justfile
}

# setup_repo: a repo whose first commit has the script and no VERSION
setup_repo() {
    command -v just >/dev/null 2>&1 || skip "just is not installed"
    command -v usage >/dev/null 2>&1 || skip "usage is not installed"
    cd "$TMP" || return
    git init -q -b main repo
    cd repo || return
    tools
    spec '-m --model <model>'
    commit "a script, no version yet"
}

@test "effort-floor: before any version commit there is nothing to compare" {
    setup_repo
    run just effort-floor
    [ "$status" -eq 0 ]
    grep -q 'no commit touches VERSION yet' <<<"$output"
}

@test "effort-floor: a plain commit after the release measures from that release" {
    setup_repo
    release 0.1.0 macro
    printf '# a comment\n' >>ask-claude
    commit "a comment"
    run just effort-floor
    [ "$status" -eq 0 ]
    grep -qE '^since [0-9a-f]{12}, version\(0\.1\.0\): macro$' <<<"$output"
    grep -q '^floor: micro' <<<"$output"
}

@test "effort-floor: a removed flag is a breaking change, an added one is not" {
    setup_repo
    release 0.1.0 macro
    spec '-e --effort <effort>'
    commit "the model flag is gone"
    run just effort-floor
    grep -q '^floor: meso' <<<"$output"
    spec '-m --model <model>' '-e --effort <effort>'
    commit "a second flag"
    run just effort-floor
    grep -q '^floor: micro' <<<"$output"
}

@test "effort-floor: the version commit at the tip measures from the version before it" {
    setup_repo
    release 0.1.0 macro
    spec '-e --effort <effort>'
    release 0.2.0 macro
    run just effort-floor
    grep -qE '^since [0-9a-f]{12}, version\(0\.1\.0\): macro$' <<<"$output"
    grep -q '^floor: meso' <<<"$output"
}

@test "effort-floor: one commit after the version commit measures from that version" {
    setup_repo
    release 0.1.0 macro
    spec '-e --effort <effort>'
    release 0.2.0 macro
    printf '# a comment\n' >>ask-claude
    commit "a comment"
    run just effort-floor
    grep -qE '^since [0-9a-f]{12}, version\(0\.2\.0\): macro$' <<<"$output"
    grep -q '^floor: micro' <<<"$output"
}

@test "effort-floor: a merge whose parent is the version commit measures from the version before it" {
    # the shape a pull_request run checks out (refs/pull/N/merge) and a
    # merge landing on main: HEAD is a merge, the version commit is its parent
    setup_repo
    release 0.1.0 macro
    git checkout -q -b pr
    spec '-e --effort <effort>'
    release 0.2.0 macro
    git checkout -q main
    printf 'notes\n' >README.md
    commit "docs on main"
    git -c user.name=t -c user.email=t@t merge -q --no-ff -m "merge pr" pr
    run just effort-floor
    grep -qE '^since [0-9a-f]{12}, version\(0\.1\.0\): macro$' <<<"$output"
    grep -q '^floor: meso' <<<"$output"
}

@test "effort-floor: a root version commit at the tip has nothing to compare" {
    command -v just >/dev/null 2>&1 || skip "just is not installed"
    command -v usage >/dev/null 2>&1 || skip "usage is not installed"
    cd "$TMP" && git init -q -b main root && cd root
    tools
    spec '-m --model <model>'
    release 0.1.0 macro
    run just effort-floor
    [ "$status" -eq 0 ]
    grep -q 'nothing to compare' <<<"$output"
}

@test "check-effver: the recipe passes a git range through" {
    setup_repo
    release 0.1.0 macro
    release 0.1.1 micro
    run just check-effver main~1..main
    [ "$status" -eq 0 ]
}

@test "check-effver: without a range in git mode the recipe prints its usage" {
    setup_repo
    run just check-effver
    [ "$status" -eq 2 ]
    grep -q 'usage: just check-effver <git range>' <<<"$output"
}

@test "effort-floor: a feature landed on a released main measures from that release" {
    # main's tip is the release; the merge brings in no release, so the floor
    # is since that release, not the one before it
    setup_repo
    release 0.1.0 macro
    spec '-e --effort <effort>'
    release 0.2.0 macro
    git checkout -q -b feature
    printf 'notes\n' >README.md
    commit "a feature"
    git checkout -q main
    git -c user.name=t -c user.email=t@t merge -q --no-ff -m "merge feature" feature
    run just effort-floor
    grep -qE '^since [0-9a-f]{12}, version\(0\.2\.0\): macro$' <<<"$output"
    grep -q '^floor: micro' <<<"$output"
}

@test "effort-floor: an octopus landing on a released main measures from that release" {
    setup_repo
    release 0.1.0 macro
    spec '-e --effort <effort>'
    release 0.2.0 macro
    git checkout -q -b f1
    printf 'one\n' >one
    commit "one"
    git checkout -q main
    git checkout -q -b f2
    printf 'two\n' >two
    commit "two"
    git checkout -q main
    git -c user.name=t -c user.email=t@t merge -q --no-ff -m "merge f1 f2" f1 f2
    run just effort-floor
    grep -qE '^since [0-9a-f]{12}, version\(0\.2\.0\): macro$' <<<"$output"
}

@test "effort-floor: when both sides carry a release, main's release is the baseline" {
    setup_repo
    release 0.1.0 macro
    git checkout -q -b pr
    spec
    spec '-m --model <model>' '-e --effort <effort>'
    release 0.3.0 macro
    git checkout -q main
    spec '-e --effort <effort>'
    release 0.2.0 macro
    git -c user.name=t -c user.email=t@t merge -q --no-ff --no-commit pr || true
    git checkout -q --theirs -- VERSION CHANGELOG.md ask-claude
    commit "merge pr"
    run just effort-floor
    grep -qE '^since [0-9a-f]{12}, version\(0\.2\.0\): macro$' <<<"$output"
}

@test "effort-floor: a release that is itself a merge measures from the release before it" {
    setup_repo
    release 0.1.0 macro
    git checkout -q -b feature
    printf 'notes\n' >README.md
    commit "a feature"
    git checkout -q main
    git -c user.name=t -c user.email=t@t merge -q --no-ff --no-commit feature
    spec '-e --effort <effort>'
    release 0.2.0 macro
    run just effort-floor
    grep -qE '^since [0-9a-f]{12}, version\(0\.1\.0\): macro$' <<<"$output"
    grep -q '^floor: meso' <<<"$output"
}

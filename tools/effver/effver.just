# EffVer recipes. The justfile pulls them in with `import 'tools/effver/effver.just'`.
# bump-effver and check-effver are copied unchanged from alycda/dotfiles@effver.

# MACRO.MESO.MICRO (0.MACRO.MICRO below 1.0); also updates the script's own copy
bump effort:
    tools/effver/bump-effver {{ effort }}
    if [ -f ask-claude ]; then sed -i.bak "s/^readonly ASK_CLAUDE_VERSION=.*/readonly ASK_CLAUDE_VERSION=$(cat VERSION)/" ask-claude && rm ask-claude.bak; fi

# Check VERSION and CHANGELOG.md in every commit of REVSET (default: the stack not on trunk yet).
# Without .jj, REVSET is a git range such as origin/main..HEAD, and there is no default.
check-effver revset='':
    if [ -d .jj ]; then tools/effver/check-effver --jj {{ quote(revset) }}; else tools/effver/check-effver {{ quote(revset) }}; fi

# The script's copy of the version matches VERSION
check-version-line:
    grep -qx "readonly ASK_CLAUDE_VERSION=$(cat VERSION)" ask-claude

# Lowest effort the next bump can claim: usage diff of the #USAGE spec since the last version
# commit. When the tip is that commit, the floor it had to meet, from the version commit before.
effort-floor:
    #!/usr/bin/env bash
    set -euo pipefail
    # tips: the parents of the jj working copy; in git, HEAD and, when HEAD is
    # a merge, its parents, since a pull_request run checks out refs/pull/N/merge
    # and a landing on main is a merge too
    if [ -d .jj ]; then
      tips='@-'
      last() { jj log --no-graph -r "latest(::($1) & files(\"VERSION\"))" -T 'commit_id'; }
      is_tip() { [ -n "$(jj log --no-graph -r "($tips) & $1" -T 'commit_id')" ]; }
      before() { echo "$1-"; }
      show() { jj file show -r "$1" "$2" 2>/dev/null; }
      subject() { jj log --no-graph -r "$1" -T 'description.first_line()'; }
    else
      tips=HEAD
      last() { git log -1 --format=%H "$1" -- VERSION; }
      is_tip() { { git rev-parse HEAD; [ -z "$(git rev-list --no-walk --merges HEAD)" ] || git rev-parse "HEAD^@"; } | grep -qx "$1"; }
      before() { echo "$1~1"; }
      show() { git cat-file -e "$1:$2" 2>/dev/null && git show "$1:$2"; }
      subject() { git log -1 --format=%s "$1"; }
    fi
    rel="$(last "$tips")"
    if [ -n "$rel" ] && is_tip "$rel"; then rel="$(last "$(before "$rel")")"; fi
    if [ -z "$rel" ]; then
      echo "no commit touches VERSION yet: nothing to compare"
      exit 0
    fi
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    mkdir "$tmp/old" "$tmp/new"
    if ! show "$rel" ask-claude >"$tmp/old/ask-claude"; then
      echo "no ask-claude at the last version commit (${rel:0:12}): nothing to compare"
      exit 0
    fi
    cp ask-claude "$tmp/new/ask-claude"
    echo "since ${rel:0:12}, $(subject "$rel")"
    if usage diff "$tmp/old/ask-claude" "$tmp/new/ask-claude"; then
      echo "floor: micro (no breaking interface change)"
    else
      echo "floor: meso (breaking interface change; macro if it hits most users)"
    fi

# EffVer recipes. The justfile pulls them in with `import 'tools/effver/effver.just'`.
# bump-effver and check-effver are copied unchanged from alycda/dotfiles@effver.

# MACRO.MESO.MICRO (0.MACRO.MICRO below 1.0); also updates the script's own copy
bump effort:
    tools/effver/bump-effver {{ effort }}
    if [ -f ask-claude ]; then sed -i.bak "s/^readonly ASK_CLAUDE_VERSION=.*/readonly ASK_CLAUDE_VERSION=$(cat VERSION)/" ask-claude && rm ask-claude.bak; fi

# Check VERSION and CHANGELOG.md in every commit of REVSET (default: the stack not on trunk yet)
check-effver revset='':
    tools/effver/check-effver --jj {{ quote(revset) }}

# The script's copy of the version matches VERSION
check-version-line:
    grep -qx "readonly ASK_CLAUDE_VERSION=$(cat VERSION)" ask-claude

# Lowest effort the next bump can claim: usage diff of the #USAGE spec since the last version commit
effort-floor:
    #!/usr/bin/env bash
    set -euo pipefail
    rel="$(jj log --no-graph -r 'latest(::@- & files("VERSION"))' -T 'commit_id.short()')"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    mkdir "$tmp/old" "$tmp/new"
    if ! jj file show -r "$rel" ask-claude >"$tmp/old/ask-claude" 2>/dev/null; then
      echo "no ask-claude at the last version commit ($rel): nothing to compare"
      exit 0
    fi
    cp ask-claude "$tmp/new/ask-claude"
    if usage diff "$tmp/old/ask-claude" "$tmp/new/ask-claude"; then
      echo "floor: micro (no breaking interface change since $rel)"
    else
      echo "floor: meso (breaking interface change since $rel; macro if it hits most users)"
    fi

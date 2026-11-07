# EffVer recipes. The justfile pulls them in with `import 'tools/effver/effver.just'`.
# bump-effver and check-effver are copied unchanged from alycda/dotfiles@effver.

# MACRO.MESO.MICRO (0.MACRO.MICRO below 1.0); also updates the script's own copy
bump effort:
    tools/effver/bump-effver {{ effort }}
    if [ -f ask-claude ]; then sed -i.bak "s/^readonly ASK_CLAUDE_VERSION=.*/readonly ASK_CLAUDE_VERSION=$(cat VERSION)/" ask-claude && rm ask-claude.bak; fi

# Check VERSION and CHANGELOG.md in every commit of REVSET (default: the stack not on trunk yet).
# Without .jj, REVSET is a git range such as origin/main..HEAD, and there is no default.
check-effver revset='':
    if [ -d .jj ]; then tools/effver/check-effver --jj {{ quote(revset) }}; elif [ -n {{ quote(revset) }} ]; then tools/effver/check-effver {{ quote(revset) }}; else echo 'usage: just check-effver <git range>, e.g. origin/main..HEAD' >&2; exit 2; fi

# The script's copy of the version matches VERSION
check-version-line:
    grep -qx "readonly ASK_CLAUDE_VERSION=$(cat VERSION)" ask-claude

# Lowest effort the next bump can claim: usage diff of the #USAGE spec since the last version
# commit. When the tip is that commit, the floor it had to meet, from the version commit before.
effort-floor:
    #!/usr/bin/env bash
    set -euo pipefail
    # The tip is HEAD in git; in jj it is the working copy when that is a merge
    # and its parent otherwise, so an empty child of a merge and the merge
    # itself agree with git. A merge's first parent is the side already
    # released: a merge measures from the last version commit there. A version
    # commit at a plain tip measures from the one before it.
    if [ -d .jj ]; then
      parents() { [ -n "$1" ] || return 0; jj log --no-graph -r "$1" -T 'parents.map(|p| p.commit_id()).join(" ")'; }
      last() { [ -n "$1" ] || return 0; jj log --no-graph -r "latest(::$1 & files(\"VERSION\"))" -T 'commit_id'; }
      show() { jj file show -r "$1" "$2" 2>/dev/null; }
      subject() { jj log --no-graph -r "$1" -T 'description.first_line()'; }
      wc="$(jj log --no-graph -r '@' -T 'commit_id')"
      case "$(parents "$wc")" in *" "*) tip="$wc" ;; *) tip="$(parents "$wc")" ;; esac
    else
      parents() { [ -n "$1" ] || return 0; git rev-list --parents -n1 "$1" | cut -s -d' ' -f2-; }
      last() { [ -n "$1" ] && git rev-parse -q --verify "$1^{commit}" >/dev/null 2>&1 || return 0; git log -1 --format=%H "$1" -- VERSION; }
      show() { git cat-file -e "$1:$2" 2>/dev/null && git show "$1:$2"; }
      subject() { git log -1 --format=%s "$1"; }
      tip="$(git rev-parse HEAD)"
    fi
    tip_parents="$(parents "$tip")"
    first="${tip_parents%% *}"
    if [ "${tip_parents#* }" != "$tip_parents" ]; then
      rel="$(last "$first")"
    else
      rel="$(last "$tip")"
      [ "$rel" != "$tip" ] || rel="$(last "$first")"
    fi
    if [ -z "$rel" ]; then
      if [ -n "$(last "$tip")" ]; then echo "no version commit before the tip: nothing to compare"; else echo "no commit touches VERSION yet: nothing to compare"; fi
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

# Findings detail

Confirmed `medium`, `high` and `critical` records, copied from `findings.json`.

## check-effver runs commands from a commit's VERSION through bash arithmetic on an unvalidated parent version

`tools/effver/check-effver:bump:parent-VERSION-arithmetic-injection`

**Root cause.** tools/effver/check-effver applies version_re (line 85) only to $version, the VERSION of the commit under check. The parent VERSION pv (line 72) goes into $old (line 75) unvalidated, and bump "$old" "$effort" (line 103) evaluates the attacker-controlled parts x/y/z in bash arithmetic (lines 56-60). The problem that line 86 records for the malformed commit does not stop the same string being used as a parent version later.

**Intended behaviour (invariant).** Only strings that match ^[0-9]+\.[0-9]+\.[0-9]+$ reach bump()'s arithmetic. A parent with a malformed VERSION produces a reported problem and no arithmetic evaluation.

### Trace

1. **entrypoint** `tools/effver/check-effver:45` (git range mode): ids=$(git rev-list --reverse RANGE): the commits in the range, authored by whoever wrote them. The --jj path at line 40 enumerates commits the same way.
2. **propagation** `tools/effver/check-effver:72` (per-commit parent loop): pv=$(show "$p" VERSION): raw tree content of the parent commit, not validated.
3. **propagation** `tools/effver/check-effver:75` (per-commit parent loop): old = highest parent version by sort -V; still the raw string (with one parent, it is that parent's VERSION).
4. **propagation** `tools/effver/check-effver:103` (bump check): want=$(bump "$old" "$effort"), reached when the child's VERSION and first heading are valid (effort set at line 94), a parent had VERSION (had=1), and the child's VERSION differs (same=0).
5. **propagation** `tools/effver/check-effver:54` (bump()): IFS=. read -r x y z: z keeps everything after the second dot, so the split does not limit the payload.
6. **sink** `tools/effver/check-effver:57` (bump()): $((z + 1)) evaluates z's value x[$(cmd)] as an arithmetic expression, and the subscript's command substitution runs. Lines 56 and 58-60 are the same sink for x and y with the other effort branches.

### Evidence

- `tools/effver/check-effver:85`: version_re is applied only to the current commit's VERSION. A mismatch appends to problems at line 86, and the for loop at line 65 continues to the next commit.
- `tools/effver/check-effver:75`: The parent VERSION is never matched against version_re before it becomes $old.
- `tools/effver/check-effver:57`: Arithmetic over unvalidated $z. Independent sandbox run (bash 5.2.21, git 2.51.1, no network) created /scratch/MARKER_v11. A valid-parent control exited 0, and a patched copy that validates parents and bump()'s argument did not create the marker (agents/v11/artifacts/poc-v11.out).
- `tools/effver/effver.just:11`: `just check-effver [revset]` runs the checker with --jj over the stack not yet on trunk by default (line 26 of check-effver), which can contain commits by other authors.

### Principal and resource

A contributor who can author commits in a range that the maintainer checks with check-effver, with no need for the maintainer to run any code from that branch.

### Input

- `VERSION in the parent commit: 0.4.x[$(touch /scratch/MARKER_v11)]`
- `VERSION in the child commit: 0.4.1, with first CHANGELOG.md heading `## 0.4.1 (micro) - 2026-11-05` and a non-empty section`

### Bounded reproduction

1. In the sandbox, create a git fixture repo with three commits on main: base (VERSION 0.4.0, heading `## 0.4.0 (macro) - 2026-11-05`), parent (VERSION set to the payload), and child (VERSION 0.4.1 with the valid micro heading).
2. Confirm /scratch/MARKER_v11 does not exist.
3. Run the unmodified target checker read-only from the fixture repo: bash /target/tools/effver/check-effver main~2..main
4. Check whether /scratch/MARKER_v11 exists. As a control, repeat with the parent VERSION 0.4.0 and expect exit 0.

### Observed output

check-effver printed `VERSION is "0.4.x[$(touch /scratch/MARKER_v11)]", not MACRO.MESO.MICRO` for the parent commit and exited 1, and /scratch/MARKER_v11, absent before the run, had been created. The control with a valid parent exited 0. A patched copy that rejects a malformed parent and makes bump() validate its argument exited 1 and did not create its marker.

### Conditions and containment

- *user_interaction*: A developer runs check-effver, with a git range or through `just check-effver` with a jj revset, over a range that contains the attacker's commits.
- *data_state*: The crafted VERSION commit is a parent of a later commit in the range. That later commit has a valid VERSION, a valid first CHANGELOG heading with a non-empty section, and a VERSION different from the parent. The attacker can author both commits. With more than one parent, the crafted string must sort highest under sort -V.
- *environmental_dependency*: Under set -u, the subscript base must be a set variable (x, y or z inside bump, or BASH_VERSINFO); an unset name aborts before the subscript expands. Reproduced on bash 5.2.21; bash 5.3 on the host was not exercised.

### Remediation and regression case

Validate every VERSION string before arithmetic. In the parent loop, report a parent VERSION that does not match version_re and do not fold it into $old. Make bump() reject an argument that does not match version_re, and treat that failure as a problem at line 103. Use 10#$n in the arithmetic only after validation, so leading zeros are not read as octal. This enforces the invariant at the sink (bump) as well as at the source, so no caller can move trust around it.

`tools/effver/check-effver`:

```bash
bump() {
  # Only a validated MACRO.MESO.MICRO string may reach arithmetic.
  [[ $1 =~ $version_re ]] || return 1
  local x y z
  IFS=. read -r x y z <<<"$1"
  case "$x:$2" in
    0:macro) echo "0.$((10#$y + 1)).0" ;;
    0:*) echo "0.$y.$((10#$z + 1))" ;;
    *:macro) echo "$((10#$x + 1)).0.0" ;;
    *:meso) echo "$x.$((10#$y + 1)).0" ;;
    *:micro) echo "$x.$y.$((10#$z + 1))" ;;
  esac
}

  # parent loop (replaces lines 71-77):
  for p in $(parents "$id"); do
    if pv="$(show "$p" VERSION)"; then
      had=1
      if ! [[ $pv =~ $version_re ]]; then
        problems+=("parent ${p:0:12} VERSION is not MACRO.MESO.MICRO")
        continue
      fi
      [ "$pv" = "$(show "$id" VERSION 2>/dev/null || true)" ] && same=1
      old="$(printf '%s\n%s\n' "$old" "$pv" | sed '/^$/d' | sort -V | tail -n 1)"
    fi
  done

  # bump check (replaces line 103):
      want="$(bump "$old" "$effort")" || want=""

# Regression fixture: base 0.4.0 -> parent VERSION 0.4.x[$(touch MARKER)] -> child 0.4.1 (micro);
# check-effver must exit 1 and MARKER must not exist.
```


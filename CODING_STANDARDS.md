# Coding standards

Rules code in this repository follows. Each one names a defect class that a
fix commit, the security audit, or a review finding showed, and that no lint
or test catches on its own. Code review checks every change against them.
Commands, setup, and anything shellcheck, bats or the hurl contracts already
enforce belong elsewhere.

## Secrets and process arguments

- **Nothing private goes on a command line.** The API key, request bodies,
  prompt text, file contents and chat lines reach curl and jq through a pipe,
  a process-substitution file (`-H @<(...)`, `--secrets-file <(...)`), stdin
  (`--data-binary @-`), or `--rawfile <var> /dev/stdin`. Other local accounts
  can read process arguments, and arguments have a size limit. Source: "keep
  the API key and request body off the command line", "request text reaches
  jq on stdin, not on its command line".
- **A key file is refused unless it is private to the user.** Treat a
  credential file the way ssh treats a private key: refuse it when another
  account owns it or can read or write it. Source: "keep the API key and
  request body off the command line".

## What reaches the terminal

- **Every string that came from the API goes through `safe` before it is
  printed.** That includes values the shell extracts itself, not only the
  jq renderers: tool names and inputs, stop reasons, event ids, raw bytes
  of a non-SSE body. `safe` keeps tab and newline and drops every other C0
  and C1 control character. Source: "strip terminal control characters
  from everything ask-claude prints", "every Managed Agents value the shell
  prints goes through safe".
- **A value shown for approval is encoded exactly once, by `jq -c`.** Never
  stack `tostring` with `@tsv` or another escaping layer; the double
  encoding displays a different command from the one being approved.
  Source: "the confirmation prompt shows the tool input encoded once".

## Values the API controls

- **An identifier from the API is validated against its shape before it
  names a file, a path segment or a URL.** Check at the sink as well as the
  source, so no caller can route around it. Source: "validate the API
  session id before it names a file", "check-effver validates parent
  VERSION before bash arithmetic".
- **Only a validated numeric string reaches bash arithmetic**, with `10#`
  so a leading zero is not octal. `$((...))` evaluates array subscripts and
  command substitutions in its operands. Source: "check-effver validates
  parent VERSION before bash arithmetic".
- **A URL gate matches the parsed authority, not a string prefix.** Refuse
  user info (`@`) outright; plain http matches the whole host and port
  against the loopback forms. Source: "--base-url gate checks the parsed
  host, refusing userinfo".

## Files and state

- **Files are created private and never written through a name that may
  already exist.** `umask 077` at the top; a download or rewrite goes to a
  `mktemp` file in the target directory and then one `mv -f`, which
  replaces a regular file only. Extending a `-e` check is not a fix,
  because check-then-use stays. Source: "agent files writes a fresh temp
  file, never through a symlink", "history files private to the user, and
  no writes through shared directories".
- **A directory other accounts can write is refused as a state location.**
  Fixed sibling names can be planted there. The audit's open lead extends
  this: the owner, a symlinked path and a failed `chmod` are not yet
  checked. Source: "history files private to the user", audit lead
  "state-dir/owner-and-parent-unchecked".
- **The existence of a fixed-name file is not a grant.** A standing
  approval is tied to the process that gave it: removed at startup,
  removed on exit, and never honoured before the no-terminal deny. PID
  reuse is deterministic in containers. Source: "the always answer to a
  tool confirmation ends with its process".
- **A failed request leaves the history as it was before the user's turn.**
  Roll back to the recorded length on any failure, including a reply jq
  cannot fold; dropping the last element is wrong after a paused turn.
  Source: "history stays consistent when a request fails or a reply cannot
  be parsed".

## jq programs

- **Every `last(...)` and `first(...)` carries an explicit default**, and
  `//` is applied outside the pipe it guards. Since jq 1.8, `last(empty)`
  yields nothing rather than null, so a missing event silently empties the
  whole fold. Source: "explicit defaults for every last(), so jq 1.8 folds
  a turn".
- **A jq program takes its inputs the way the oracle passed them**
  (`--arg`, `--argjson`, `--rawfile`, `--slurpfile`, same names), so the
  goldens stay valid and the Rust port has one shape to match. Source:
  `docs/plan.md`, "jq file contract".
- **A redaction filter matches values, not keys**, and lists every id
  prefix the API can emit. Source: "redact ids as values only, and
  agent_name, file_ and memstore_ ids".

## Tests

- **A regression test fails on the parent commit and passes on the fix.**
  The commit message says so. A test that passes against the old script
  has not shown anything. Source: every `fix:` commit since the audit;
  "argv.bats ... reworked until it discriminated".
- **A live-path failure the replay seam skips gets a fixture that replays
  it.** The seam branches before curl, so bats cannot see a failure on the
  live side unless a fixture carries it. Source: "sessions survive the live
  paths the replay seam skips".
- **A negative assertion in bats goes through a helper.** A bare `! grep`
  does not fail the test unless it is the last line. Source: "request text
  reaches jq on stdin" (on the way).
- **A gate passes at zero failures.** A test that cannot pass in some
  environment is excluded there by name or tag, never tolerated as a
  count. A steady red baseline hides new failures: two tests that caught
  a real fold bug sat among twenty-four fixture failures on every fresh
  checkout until found by accident. Source:
  `docs/solutions/workflow-issues/a-steady-red-baseline-hides-new-failures.md`.
- **A recorded fixture is tracked, and its redaction is tested without the
  API.** A gitignored fixture hides the failures of every test that needs
  it. Source: "redact ids as values only", "track the session fixtures".

## Maintaining this file

A defect class enters this file only when it appeared in a review finding
the author fixed, a fix commit, a cleanup that removed it across the
codebase, or a solution record, and no check covers it. State the class as
a condition in a few sentences that stay true without naming volatile
numbers or symbols, and group rules by area. When a lint rule or test
starts to catch a class, retire its rule here.

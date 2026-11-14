---
title: A steady red baseline hides new failures
date: 2026-11-06
category: workflow-issues
module: tests
problem_type: workflow_issue
component: testing_framework
severity: medium
applies_when:
  - "A test suite is accepted with a known number of failures rather than zero"
  - "A gate or version commit records a count such as 153/154 instead of a named exclusion"
  - "Fixtures are gitignored by pattern with tracked exceptions"
  - "A new test fails and the first guess is that the test is wrong"
tags: [bats, fixtures, gitignore, test-baseline, verifier-gate, jq]
---

# A steady red baseline hides new failures

## Context

Every fresh checkout of this repository, one without the locally recorded
session streams, ran the unit suite red until the fixture commit landed on
2026-11-05. The red count moved as session tests were added, 19 then 24,
and stood at 128 passing and 26 failing at the end. Twenty-four of the
failures came from the Managed Agents session fixtures: `.gitignore` ignores
`*.sse` with tracked exceptions, and the only exception at the time covered
the Messages streams under `tests/fixtures/sse/`. The session streams under
`tests/fixtures/sessions/` matched the ignore pattern and were never
committed, on any branch. Every test that read one failed before it reached
the code it tested. Checkouts that still had the local recordings were
green, which is why the commits of that period record counts like 139/140.

The other two failures were real. Two inline tests in `tests/unit/session.bats`,
"an unknown event type prints nothing and folds to nothing" and "a stream
without session.usage folds to zero usage, not null", had been catching a
fold bug since jq was pinned to 1.8.2: `last(empty)` yields nothing in jq
1.8 where 1.7 gave `null`, so a session stream without a `session.error`
event folded to no output at all, and the fix commit records that a
`requires_action` turn was then never answered. Nobody saw the two, because
on a fresh checkout the suite was already red and the count did not move in
a way anyone was watching.

The bug was found sideways, while writing a regression test for a different
fix during the security audit: the new test could not reach its target
without the fold working. The first guess was that the new test was wrong,
not the code, because a failing test in a red suite carries no signal.
(session history)

## Guidance

A gate passes at zero failures. When a test cannot pass in some environment,
exclude it there by name or by tag, so the suite still reports zero and the
exclusion is visible in the command, not in a tolerated count.

In the Linux devcontainer where the gates run, the one such test is the
bracketed-paste check in `tests/unit/chat.bats`, which asserts that the bash
running the chat loop is 5.1 or newer with bracketed paste on. It cannot pass
under macOS `/bin/bash` 3.2, and that is the point of the test. It also
fails whenever `TERM` is unset or `dumb`, with or without a terminal: readline
reports bracketed paste from the terminal name, so a pty alone does not make
it pass and a bare bats run with a capable `TERM` does. The version commits
from 0.2.0 to 0.2.4 record the failure as "N/N+1 (the bracketed-paste pty
test)", which names the wrong cause. On macOS a second test was bound to the
environment: the download-mode check in `tests/unit/agent-files.bats` used
GNU `stat -c`, which BSD stat does not have. It now falls back to `stat -f`.

The exclusion belongs in the suite command, and it is there now. The test
carries a bats tag, `just unit-tests` leaves the tag out, and `just pty-test`
runs the tagged test under a terminal name readline accepts. The Rust port
adds a second tag, `rust-only`, for behaviour the script does not have.

```bash
# above the test in tests/unit/chat.bats
# bats test_tags=pty

# just unit-tests: the default suite, zero failures expected
bats --filter-tags '!pty,!rust-only' tests/unit

# just pty-test: the excluded test alone
TERM=xterm-256color bats --filter-tags pty tests/unit
```

When fixtures are gitignored by pattern, the harness test should fail on a
missing fixture with a message that says so, and it does now: `present()` in
`tests/unit/00-harness.bats` names the missing file, where a bare `-s` check
failed without one. The exceptions should be reviewed whenever a new fixture directory is
added, and the check that every fixture is tracked is one command: run from
the repository root, `git check-ignore -v tests/fixtures/sessions/*/*.sse
tests/fixtures/sessions/*.sse` must print nothing. The harness comment in
`tests/unit/00-harness.bats` records which session fixtures are live
recordings and which are still derived.

## Why This Matters

A red baseline turns every new failure into noise. The count stays "about the
same", the gate wording becomes "N known failures", and the next real failure
is one more line in a list nobody reads. The two fold-bug tests were green on
jq 1.7 and red on the pinned 1.8.2, which is exactly the regression a pinned
toolchain exists to catch, and a fresh checkout could not show it.

It also inverts the first instinct when a new test fails. In a green suite a
red test means the code is wrong until shown otherwise. In a red suite the
author suspects the test first, which is what happened here, and the real
cause was found only because the test could not be made to pass without it.

## When to Apply

- Before recording a gate result or a version commit: the suite command is
  the one that reports zero, with any exclusion named in it.
- When a test is known not to pass in an environment: tag it and exclude the
  tag there, rather than noting the failure in prose, and name the real
  cause.
- When adding a fixture directory under a gitignored pattern: add the
  exception in the same commit and confirm with the ignore check above that
  every fixture is tracked.
- When a new test fails in a suite that is already red: fix the baseline
  before judging the test.

## Examples

What the version commit said at 0.2.4:

```text
bats tests/unit    153/154 (the bracketed-paste pty test)
```

What it would say with the exclusion in the command, at the same point:

```text
bats --filter-tags '!pty' tests/unit                        153/153
TERM=xterm-256color bats --filter-tags pty tests/unit/chat.bats   1/1
```

The fixture side, as it happened: the recordings were lost, so the fifteen
session streams were reconstructed by hand. The eight that have goldens
reproduce their `.stream.txt` and `.fold.json` byte for byte under jq 1.8.2
and jq 1.7; the other seven are shaped by the tests that read them. The
reconstruction proved the suite, not the live wire format, so the streams
were re-recorded live for 0.2.5, ids redacted. `budget` and `error` are
derived from `text` by hand, and three streams (`not-sse/1`, `terminated/1`,
`waiting-unknown/1`) are still reconstructed with a live stream's framing.

## Related

- `CODING_STANDARDS.md`, Tests: the rule this learning explains, next to the
  rule that fixtures are tracked and their redaction tested offline.
- `tests/unit/00-harness.bats`: the comment on which session fixtures are
  recorded and which are derived.
- The fix commit "explicit defaults for every last(), so jq 1.8 folds a
  turn" and the fixture commit "track the session fixtures, reconstructed
  from their goldens", both reachable from this tree.

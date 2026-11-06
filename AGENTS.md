# ask-claude: instructions for agents

ask-claude is a shell client for the Anthropic Messages API and Managed
Agents sessions. The bash script `ask-claude` is the reference. A Rust port
lives in `rust/` on its own line and replaces the script when it reaches
parity. `docs/plan.md` is the contract: settled decisions, the commit graph,
and one row per commit with its validation. Read it before any change.

## The loop: plan, fail, build, verify

Every step leaves a commit, so the work can be reviewed from history alone.

1. **Plan.** The step is a row in `docs/plan.md`. An unbuilt step is an
   empty `TODO:` commit whose description is the instruction. Each milestone
   ends in an empty `TODO(verifier):` commit listing the gate commands in
   order.
2. **Fail.** A `test:` commit comes first and is red on its parent for
   exactly the new tests. Unit tests are bats over recorded fixtures and
   goldens (`just unit-tests`); live contracts are hurl (`just test`,
   `just test-openai`). Goldens are what the oracle does today; change one
   only in a commit that says why the oracle was wrong.
3. **Build.** The `feat:` commit is the child of its test commit and turns
   it green with no edit to the test. If the test must change, that is its
   own commit with the reason.
4. **Verify.** Run the gate from the `TODO(verifier):` commit, in order, and
   record it as an empty `verified(x.y.z): the gate passed on <workspace>@
   <date>` commit whose body lists every command and its result. Then
   `version(x.y.z):` bumps with `just bump`, a CHANGELOG section, and
   `just check-effver`. EffVer labels the effort to adopt, never a milestone;
   `just effort-floor` names the lowest label a release can claim.

## Ownership

Agents write tests, fixtures, goldens, scaffolds, the commit plan and TODO
placeholders. The maintainer writes the implementation and the version bumps.
Agents work in their own jj workspace, never on `@` in `default`, and never
push unless told.

## History

This is a jj repository. Independent lines are siblings joined by a merge;
chain only true dependencies. Commit messages follow Chris Beams' seven
rules and say why, including the alternative not taken. Never put a jj
change ID or a commit hash in a message; name other commits by subject.
Edit an existing commit through a child and `jj squash`. After any rewrite,
check the range for conflicts and divergence. A wrong turn that taught
something stays in history; squash only fixups with no lesson.

## Where things are

- `docs/plan.md` — decisions, commit graph, per-commit validation and
  status.
- `docs/plans/` — implementation plans written by `ce-plan`.
- `docs/security-audit/` — the audit report and findings; each finding was
  fixed in its own commit with a regression test.
- `tests/fixtures/` — recorded SSE and session streams, ids redacted, and
  their goldens. Re-record live only when a `TODO:` commit says so.
- `tools/effver/` — `bump`, `check-effver`, `effort-floor`.
- `rust/` — the port, phase 4 of the plan.

## Providers

Venice is a base URL, not a provider. `-p openai` is refused in bash; the
OpenAI chat-completions shape belongs to the Rust port's
`OpenAiCompatible { base_url }`. `--base-url` must be https, or http on
loopback.

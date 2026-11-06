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

After a solved, verified problem, offer once to invoke the `ce-compound` skill at the completion checkpoint only when the work produced durable project reasoning that is not readily recoverable from the final code, tests, types, comments, or existing documentation, and losing it would plausibly cause recurrence, material risk, or substantial rediscovery. Apply this counterfactual: if the learning document disappeared, would a future engineer reading the final implementation still be likely to repeat the mistake or redo substantial investigation? If not, do not offer. Completion, effort, and diff size alone are not enough. Offer at the checkpoint so a qualifying learning can ship in the PR that produced it, and only where the repository treats captured learnings as tracked, committed knowledge.

## History

This is a jj repository. Independent lines are siblings joined by a merge;
chain only true dependencies. Commit messages follow Chris Beams' seven
rules and say why, including the alternative not taken. Never put a jj
change ID or a commit hash in a message; name other commits by subject.
Edit an existing commit through a child and `jj squash`. After any rewrite,
check the range for conflicts and divergence. A wrong turn that taught
something stays in history; squash only fixups with no lesson.

## Where things are

- `CODING_STANDARDS.md` — the rules code here follows. Read it before
  writing or reviewing code; update it when a change establishes or
  retires a rule.
- `REVIEW.md` — how review behaves here: what a finding needs, the
  severities, what not to flag.
- `CONCEPTS.md` — shared domain vocabulary (entities, named processes,
  status concepts). Relevant when orienting to the codebase or discussing
  domain concepts.
- `docs/plan.md` — decisions, commit graph, per-commit validation and
  status.
- `docs/plans/` — implementation plans written by `ce-plan`.
- `docs/solutions/` — documented solutions to past problems (bugs, best
  practices, workflow patterns), organized by category with YAML
  frontmatter (module, tags, problem_type). Relevant when implementing or
  debugging in a documented area.
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

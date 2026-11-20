---
title: "feat: fleet, durable fire-and-review agent runs on Temporal, behind a cargo feature"
type: feat
date: 2026-11-19
---

# feat: fleet, durable fire-and-review agent runs on Temporal, behind a cargo feature

## Summary

Add `ask-claude fleet`, built only with `--features fleet`. Each task, a
prompt or a `TASK.md`, becomes one Temporal workflow whose id is the task's
id. Each attempt is one turn of one Managed Agents session. Between attempts
the workflow waits for a review: approve, or reject with notes that become
the next turn. A durable timer races the review, and a budget and an attempt
cap bound the cost. The default build is unchanged: same dependency tree,
same usage spec, same binary.

This is the fire-and-review workflow that ran from May to July 2026, moved
from the stalled dotfiles `fleet` work into the client it drives. It is also
the homework for Temporal Replay 2027: the Agentic SDLC workshop (durable
pipelines, generate/test/fix loops with cost ceilings, approval gates) and
the Workflow Versioning workshop (event history, replay, patching).

---

## Problem Frame

The original workflow fired headless agents in parallel and reviewed their
output. Its state lived in a terminal multiplexer and the reviewer's head. A
crashed process, a closed laptop or a forgotten pane lost the run: what was
attempted, what it cost, what the reviewer said. The August 2026 billing
change ended headless runs, and the zellij rebuild (`fleet` in dotfiles) is
stalled until year end.

ask-claude already has the pieces a run needs: Managed Agents sessions with a
budget cap, a sandbox and a transcript kept by the API (`agent`), and a
private state directory. It lacks a durable control loop *around* sessions:
one that survives the process, waits days for a review without holding a
thread, and refuses to spend past a ceiling. That loop is what Temporal is
for, and the workflow code lives in the same repository as the client it
drives.

Not a goal: a Temporal-backed `ask-claude agent`. A Managed Agents session
already keeps its own transcript and budget on Anthropic's side, so wrapping
a single session in a workflow would duplicate it. fleet's value is the loop
across turns: review, retry with notes, ceiling, close.

---

## Requirements

**Build**

- R1. `cargo build` without the feature produces the same dependency tree and
  the same usage spec as before this plan: `just check-spec` and the `cargo
  tree` snapshot are unchanged.
- R2. `cargo build --features fleet` adds one subcommand, `fleet`, and needs
  nothing on `PATH` that the default build does not (no `protoc`).
- R3. CI builds, lints and tests both feature sets.

**Tasks**

- R4. `ask-claude fleet run <prompt | path/to/TASK.md>` starts one workflow
  per task. A `TASK.md` is read in the dotfiles `tasks/` format (`STATUS`,
  `TAGS`, `## Description`); its directory name is the task id. A prompt
  gets a fresh HUID (`YYYYMMDD-HHMMSS`, UTC) as its id.
- R5. A `TASK.md` with `STATUS: CLOSED` is refused with exit 2. Starting a
  task whose id already has a running workflow is refused with exit 2, naming
  the running workflow.

**Attempts**

- R6. Attempt 1 creates a Managed Agents session for the task, with the
  fleet budget as the session's own budget cap, and sends the task's text as
  the first message. Attempt n > 1 sends the reviewer's notes as the next
  user message on the same session.
- R7. Each attempt's stream is written, through the same printer and `safe`
  as the terminal path, to `fleet/<id>/<n>.stream.txt` under the state
  directory (mode 600). Nothing is printed to the worker's stdout.
- R8. A worker that dies mid-attempt does not send the attempt's message
  twice. The retried activity reattaches to the session and waits for idle.

**Review**

- R9. After each attempt the workflow waits for `ask-claude fleet review
  <id> approve`, or `reject` with notes from `-f <file>` or stdin. A review
  timeout (default 72h) races it. On timeout the workflow ends `expired`
  and changes nothing.
- R10. On approval of a `TASK.md` task, `STATUS` becomes `CLOSED`, written
  through a temp file and rename as `history::replace` writes chats.

**Cost**

- R11. The workflow refuses to start an attempt when the session's reported
  spend has reached the fleet budget, or when the attempt count has reached
  the cap (default 5). It ends `exhausted`. The session's own cap (R6) is
  the backstop if the workflow's view of spend is stale.

**Privacy**

- R12. Workflow history holds ids, paths, counts, cents and outcomes. It
  holds no prompt, reply, notes or file contents. The prompt and the notes
  are files in `fleet/<id>/` (mode 600), and the history carries their
  paths. This is the event log's rule (R6 of the event-log plan) applied to
  a second store.

**Operation**

- R13. `fleet worker` runs a worker until Ctrl-C, draining in-flight
  activities within a grace period. `fleet status <id>` prints a task's
  attempt, spend, session and state; `fleet list` prints the open tasks.
- R14. Workflow code is checked against recorded histories offline in
  `cargo test --features fleet`. A history that no longer replays fails the
  test.

---

## Key Technical Decisions

- **A cargo feature, not a second crate (for now).** `fleet = ["dep:temporalio-sdk",
  "dep:temporalio-client", "dep:temporalio-macros", "dep:tokio"]`, off by
  default. Every Temporal type stays in `rust/src/fleet/`, behind
  `#[cfg(feature = "fleet")]`, with fleet's own types at the boundary, so an
  SDK upgrade touches one module. Not taken: a workspace member
  `ask-claude-fleet` binary. It isolates compile time and the lockfile
  further, but costs a second binary to install and a public library surface.
  Revisit if the feature's build time or `Cargo.lock` churn hurts the
  default build.
- **`vendored-protox`, not `protoc`.** `temporalio-protos` compiles its
  protobufs at build time with `protoc` by default. Enabling
  `temporalio-client/vendored-protox` compiles them with the pure-Rust
  `protox` instead, so R2 holds and `mise.toml` gains no build tool for it.
- **SDK default features off.** `temporalio-sdk` defaults to `envconfig` and
  `prometheus`. Keep `envconfig` (`TEMPORAL_ADDRESS` and `temporal.toml`, the
  same configuration the `temporal` CLI reads); drop `prometheus`. Exact
  pins as for every dependency (KTD20): `temporalio-sdk`,
  `temporalio-client` and `temporalio-macros` `=1.2.0` at the time of
  writing, `tokio` exact. The tree is reviewed in the commit that adds them.
  Measured on 2026-11-19: the default build's tree is 110 crates;
  `temporalio-sdk` and `temporalio-client` alone are about 290.
- **Async lives only inside `fleet`.** The crate is synchronous (`ureq`, no
  runtime). `fleet worker` and the client subcommands build a tokio runtime
  in their own `main` branch. Activities call the existing synchronous
  library on `spawn_blocking` and heartbeat from the async side while it
  runs. No other module gains `async`.
- **One session per task, one turn per attempt.** A rejection's notes go to
  the same session, so the agent keeps its context and its sandbox's files.
  Not taken: a fresh session per attempt with the notes prepended. That is
  simpler to retry but loses the sandbox state, and spend is no longer one
  number to read. The session id is recorded in workflow state after attempt
  1.
- **The send is marked before it happens.** `run_attempt` writes
  `fleet/<id>/<n>.sent` (holding the session id) after the API accepts the
  `user.message`. A retried activity that finds the marker opens the
  session's stream and waits for idle instead of sending. This is the
  Managed Agents reconnect rule (a new stream, then the history, then skip
  the events already seen) applied across processes. Without it, R8 fails
  the first time a worker dies between the send and idle.
- **Retries by outcome, not by exit code.** The library's `i32` exit codes
  cannot tell a 529 from a bad agent id. U1 returns a typed turn outcome.
  Non-retryable `ApplicationFailure`s: bad input, budget reached, session
  terminated, session error. Retryable: no response, a stream that ends
  before idle, rate limit and overload. The policy: 3 attempts, 10 s
  initial, backoff 2. Heartbeat timeout 60 s, heartbeats every 10 s, with
  the count of events seen as progress.
- **Review is an Update, the deadline a timer.** `#[update] review(Verdict)`
  sets state. The run loop is `workflows::select!` between
  `ctx.wait_condition(|s| s.verdict.is_some())` and `ctx.timer(review_timeout)`.
  An Update, not a Signal, so `fleet review` learns that the verdict was
  accepted (and refused when no review is pending) before it exits. Use the
  SDK's deterministic `select!`, never `tokio::select!` (the SDK's runtime
  detector flags the latter; `futures::select!` without `biased` slips past
  it).
- **Notes travel as a path.** `fleet review <id> reject` writes the notes to
  `fleet/<id>/<n>.notes.md` (mode 600) and sends that path in the Update.
  The update handler cannot do I/O, and R12 keeps the text out of history.
- **Workflow id is the task id; one task queue.** Id-conflict policy is
  "fail", which is R5's duplicate refusal for free. The task queue is
  `fleet` (configurable), not `ask-claude`, so the planned rename to ask-ai
  does not move running workflows.
- **The usage spec describes the default build.** `ask-claude.usage.kdl` and
  `check-spec` stay feature-less (R1). `fleet` is documented in the README
  and `fleet --help`. See Open Questions for a second snapshot.
- **Local server, persisted.** The development target is `temporal server
  start-dev --db-filename <state>/fleet/temporal.db`. Without
  `--db-filename` the dev server keeps state in memory and a restart loses
  every waiting review. The `temporal` CLI is pinned in `mise.toml`, and
  the SDK's test server is pointed at that binary instead of downloading
  its own.

---

## High-Level Technical Design

Directional, not compilable. Names are the Rust SDK 1.2.0's.

```text
fleet run TASK.md ──start(id=HUID)──▶ Temporal ◀──poll── fleet worker
fleet review id ───update(review)──▶    │                 ├─ TaskWorkflow (deterministic)
fleet status id ───query(status)───▶    │                 └─ activities (sync lib on spawn_blocking)
                                        │                      run_attempt ─▶ Managed Agents session
                                        ▼                      close_task  ─▶ TASK.md
                                  history (ids, paths,
                                  cents; no text)        state dir: fleet/<id>/{prompt.md,
                                                           <n>.stream.txt, <n>.sent, <n>.notes.md}
```

```rust
#[workflow]
pub struct TaskWorkflow { task: TaskRef, attempt: u32, session: Option<String>,
                          spent_cents: u64, verdict: Option<Verdict>, limits: Limits }

#[workflow_methods]
impl TaskWorkflow {
    #[run]
    async fn run(ctx: &mut WorkflowContext<Self>) -> WorkflowResult<Outcome> {
        let mut input = ctx.state(|s| s.task.input_path());       // prompt.md or TASK.md
        loop {
            if ctx.state(|s| s.over_budget() || s.at_cap()) { return Ok(Outcome::Exhausted) }
            let n = ctx.state_mut(|s| { s.attempt += 1; s.verdict = None; s.attempt });
            let out = ctx.execute_activity(Activities::run_attempt,
                        AttemptIn { id, n, session: ctx.state(|s| s.session.clone()), input },
                        run_attempt_options())?.await?;
            ctx.state_mut(|s| { s.session = Some(out.session); s.spent_cents = out.spent_cents });
            select! {
                _ = ctx.wait_condition(|s| s.verdict.is_some()) => {}
                _ = ctx.timer(ctx.state(|s| s.limits.review_timeout)) => return Ok(Outcome::Expired),
            }
            match ctx.state(|s| s.verdict.clone().unwrap()) {
                Verdict::Approve => { close_if_task_md(ctx).await?; return Ok(Outcome::Approved) }
                Verdict::Reject { notes_path } => input = notes_path,
            }
        }
    }
    #[update] fn review(&mut self, _ctx: &mut WorkflowContext<Self>, v: Verdict) -> Result<(), Refused>;
    #[query]  fn status(&self, _ctx: &WorkflowContextView) -> Status;
}
```

History per attempt is about a dozen events (activity scheduled, started,
completed; update accepted, completed; timer started, cancelled; workflow
task events). With the default cap of 5, a task stays far under the
51,200-event limit, so no Continue-As-New.

---

## Scope Boundaries

### Deferred to follow-up work

- **zellij panes.** `fleet worker` in one pane and each attempt's stream
  file followed in another. It is a viewer over R7's files and needs nothing
  from the workflow. A separate feature (`zellij`), its own plan.
- **The OpenAI-compatible runner.** Venice-backed attempts need an agent
  loop on a stateless API: the 1.1.0 local tool loop (`--tools`) on the
  Messages path, then tool calls in `openai.rs`, which today sends text
  only. When both exist, `run_attempt` gains a second runner and the
  workflow does not change. Durability for that loop comes from the
  workflow, since the provider keeps no session.
- **A Messages-path runner** without tools: possible now, but fire-and-review
  needs a sandbox, so it waits for the same 1.1.0 loop.
- **Interrupt on cancel.** `fleet cancel <id>` cancelling the workflow and
  the activity sending `user.interrupt`, as Ctrl-C does in `agent`.
- **Several machines.** Paths in history are the worker's. Workers on
  another host, Temporal Cloud and mTLS are out until there is a second
  machine to run them.
- **The event log.** Session turns already append one line each. A `fleet`
  id on those lines is a later, separate change to the log's schema.
- **dotfiles adoption.** `just task` in dotfiles makes `TASK.md` files that
  fleet already reads (R4). Wiring a `just fleet` recipe there waits for the
  dotfiles work to resume.

### Outside this product's identity

- Running or packaging a Temporal server. fleet connects to one; the README
  says how to start the dev server.
- A dashboard. Temporal's web UI (`:8233` on the dev server) is the view of
  every workflow, its history and its pending updates.

---

## Implementation Units

Each `test:` commit is red on its parent for exactly its new tests, and its
`feat:` child turns it green with no test edit (`AGENTS.md`). C writes tests,
fixtures and scaffolds; A writes the implementation. When this plan is
settled, it becomes a phase in `docs/plan.md` with its commit graph.

### U1. A session turn as a library function

- **Goal:** One function that runs one session turn (create or reattach,
  send unless already sent, stream to idle) writing to supplied writers and
  returning a typed outcome. `agent` calls it with stdout and stderr, so
  nothing it prints changes.
- **Requirements:** R6, R7, R8, and the retry decision.
- **Dependencies:** none. Not behind the feature: the extraction is a
  refactor of `agent.rs`.
- **Files:** `rust/src/agent.rs`, `rust/src/session.rs` or a new
  `rust/src/turn.rs`, `rust/tests/agent.rs`.
- **Approach:** lift the turn loop out of the private chat and one-shot
  paths. `TurnOutcome { session, stop: Stop, spent_cents, events_seen }`,
  where `Stop` separates idle with `end_turn`, budget reached, terminated,
  session error, no response and a stream that ended early. Heartbeat
  progress comes from a callback per event, so the activity can report it.
- **Test scenarios:** every existing session golden is byte-identical
  through the new function; each recorded fixture maps to its `Stop`; a
  turn writing to a `Vec<u8>` produces the `.stream.txt` golden; with
  `sent = true` no `user.message` reaches the request log.
- **Verification:** `just rust-test`, `just unit-tests` green; the agent
  goldens untouched.

### U2. The feature, its dependencies, CI

- **Goal:** `--features fleet` compiles a `fleet` module that is
  `todo!()` bodies, and the default build is provably unchanged.
- **Requirements:** R1, R2, R3.
- **Dependencies:** none (beside U1).
- **Files:** `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/src/fleet/mod.rs`,
  `rust/src/cli.rs` (the `Fleet` variant under `cfg`), `justfile`
  (`build-fleet`, `test-fleet`), the CI workflow, `mise.toml` (the
  `temporal` CLI), a `cargo tree` snapshot for the default build.
- **Approach:** the dependencies in their own commit with the tree reviewed
  in its message (KTD20), then the feature wiring. The CI matrix is
  `{default, fleet}` × `{lint, test}`.
- **Test scenarios:** the default `--usage-spec` equals the snapshot; the
  default `cargo tree -e normal` equals its snapshot; `ask-claude fleet` on
  a default build is an unknown subcommand (exit 2); a fleet build with no
  `protoc` on `PATH` succeeds.
- **Verification:** both CI legs green; `just check-spec` unchanged.

### U3. Tasks and the state directory

- **Goal:** Pure functions: read a `TASK.md`, make a HUID, lay out
  `fleet/<id>/` privately, write a prompt or notes file.
- **Requirements:** R4, R5 (the CLOSED half), R12.
- **Dependencies:** U2.
- **Files:** `rust/src/fleet/task.rs`, `tests/fixtures/fleet/tasks/*`,
  `rust/tests/fleet_task.rs`.
- **Approach:** the `TASK.md` grammar is the dotfiles `tasks/README.md` one;
  unknown header lines are ignored and kept. Directory and files go through
  `history::private_state` and the same `fresh`/`replace` helpers as chats.
- **Test scenarios:** OPEN and CLOSED fixtures; a missing `STATUS` line is
  refused; `TAGS` empty and absent; a HUID matches `[0-9]{8}-[0-9]{6}`; the
  directory is 700 and files 600; a symlinked `fleet/` is refused as the
  state directory is; the `STATUS` rewrite changes that line only.
- **Verification:** `cargo test --features fleet fleet_task`.

### U4. Activities

- **Goal:** `run_attempt` and `close_task`, with R8's send marker and the
  retry classification.
- **Requirements:** R6, R7, R8, R10, R11 (the spend it reports).
- **Dependencies:** U1, U3.
- **Files:** `rust/src/fleet/activities.rs`, `rust/tests/fleet_activities.rs`.
- **Approach:** `spawn_blocking` around U1's function with a file writer;
  an async heartbeat loop beside it. `Stop` maps to retryable or
  non-retryable `ApplicationFailure` per the decision. Tests use the SDK's
  `testing` feature (`ActivityEnvironment`) with `ASK_CLAUDE_REPLAY`
  session fixtures, so they stay offline.
- **Test scenarios:** a text fixture writes `1.stream.txt` equal to its
  golden and returns its spend; a `budget.sse` fixture is non-retryable; a
  stream cut before idle is retryable; with `1.sent` present, the request log
  shows no `user.message` and the stream file is completed; `close_task`
  leaves every line but `STATUS` byte-identical.
- **Verification:** `cargo test --features fleet fleet_activities`.

### U5. The workflow and its replay fixtures

- **Goal:** `TaskWorkflow` as designed above, and recorded histories for
  every path through it.
- **Requirements:** R9, R11, R14.
- **Dependencies:** U4.
- **Files:** `rust/src/fleet/workflow.rs`,
  `tests/fixtures/fleet/histories/{approve,reject-approve,expired,exhausted}.json`,
  `rust/tests/fleet_replay.rs`, a recording recipe in `justfile`.
- **Approach:** histories are recorded once against the dev server with
  replayed activities (the session fixtures) and short timeouts, then
  committed. `WorkflowReplayer` runs each one in `cargo test`. Like the SSE
  goldens, a history changes only in a commit that says why the old one was
  wrong.
- **Test scenarios:** each history replays; a `review` with no review
  pending is refused; budget reached after attempt 2 ends `exhausted`
  without scheduling attempt 3; the timer winning ends `expired` and
  schedules no `close_task`.
- **Verification:** `cargo test --features fleet fleet_replay`.

### U6. The commands

- **Goal:** `fleet worker | run | review | status | list`.
- **Requirements:** R4, R5, R9, R13.
- **Dependencies:** U5.
- **Files:** `rust/src/fleet/cli.rs`, `rust/src/cli.rs`, `README.md`.
- **Approach:** connection from `envconfig` (`TEMPORAL_ADDRESS`, the
  `temporal` CLI's profile file). `run` writes `prompt.md` for a prompt
  before starting. A start that hits an existing id is exit 2 naming it.
  `list` filters workflows by type and running status.
- **Test scenarios:** parse tests for each subcommand; `review` with `-f` and
  with stdin; exit codes 0, 1 (server unreachable), 2 (bad input, duplicate,
  closed task).
- **Verification:** parse tests green; the live gate in U8.

### U7. Break it on purpose

- **Goal:** the Versioning-workshop exercise, kept as a test. Change the
  workflow while a recorded execution is waiting for review, see the
  non-determinism error, fix it with `ctx.patched`.
- **Requirements:** R14.
- **Dependencies:** U5.
- **Files:** `rust/src/fleet/workflow.rs`,
  `tests/fixtures/fleet/histories/pre-patch-*.json`,
  `rust/tests/fleet_replay.rs`, a `docs/solutions/` note if it qualifies
  under the `ce-compound` rule.
- **Approach:** a real change worth having, for example a `run_checks`
  activity (the task's test command) between the attempt and the review,
  added behind `ctx.patched("run-checks")`. Commit the unpatched change's
  failing replay as the `test:` commit's red, then the patch as its green.
- **Test scenarios:** pre-patch histories replay on the old branch; new
  executions take the new branch; removing the `patched` call fails the
  pre-patch replays.
- **Verification:** `cargo test --features fleet fleet_replay`.

### U8. The live gate

- **Goal:** one live run of each path against a real dev server and real
  sessions, as `just test-agents` does for `agent`.
- **Requirements:** all.
- **Dependencies:** U6, U7.
- **Files:** `justfile` (`test-fleet`), `README.md`.
- **Approach:** `just test-fleet` starts the pinned dev server on a temp
  database, a worker, then a task through approve and a task through reject
  then approve, with a 50-cent budget, and archives the sessions after.
- **Verification:** the gate below, recorded as a `verified` commit.

Gate, in order: `just lint`, `just rust-test`, `just unit-tests`, `just
check-spec`, `cargo test --features fleet`, `just test-fleet`; then by hand:
kill the worker mid-attempt and see the attempt finish once on restart, and
restart the dev server while a review is pending and see it still pending.

Version: an opt-in feature with the default build unchanged, so `just
effort-floor` says micro. The label is the maintainer's.

---

## Risks & Dependencies

- **The Rust SDK is young.** Its repository called it public preview in
  2026, though the crates are versioned 1.x. Expect API changes between
  minors. Mitigation: exact pins, every SDK type in `src/fleet/`, the replay
  tests as the canary on upgrade.
- **Tree size.** The feature roughly triples the dependency tree (tonic,
  prost, tokio and their trees). Only feature builds pay for it, and KTD20's
  review applies once, in the commit that adds them.
- **Managed Agents is in beta**, as in the sessions plan. The recorded
  fixtures and `just test-agents` stay the canary, now for fleet too.
- **Spend lags.** The workflow sees spend after an attempt, not during it.
  The session's own cap (R6) is what stops a runaway attempt.
- **A dev server without `--db-filename`** loses waiting reviews on
  restart. The README and `test-fleet` both pass it.
- **One machine.** Paths in history are only meaningful to a worker on the
  machine that wrote them. Fine until there is a second worker host.

---

## Open Questions

- Should a fleet build carry its own usage spec snapshot
  (`ask-claude.fleet.usage.kdl`) so completions cover `fleet`, or is
  `--help` enough until the feature is on by default?
- Does `review` belong on the workflow (one Update per verdict) or as a
  child workflow per attempt, so each attempt's history is separate in the
  UI? The plan assumes the former: fewer moving parts, same replay story.
- What should a rejected attempt's notes look like to the agent: the notes
  verbatim, or wrapped with the attempt number and the task's title? This
  decides what U1 sends and is cheap to change behind a patch (U7).
- Default review timeout: 72h fits "fire on Friday, review on Monday"; is
  that the workflow, or should an expired task re-arm instead of ending?

---

## Assumptions

- The `temporal` CLI can be pinned in `mise.toml` through mise's GitHub
  backend (`github:temporalio/cli`) with the lockfile's four platforms. If
  not, the README names the version and the SDK's test server downloads it.
- The session's spend is readable after a turn the way `agent list` reads it
  (`GET /v1/sessions/{id}`).
- Reattaching to a session whose turn is still running, by opening a new
  stream, delivers that turn's remaining events and its idle. The sessions
  plan's reconnect rule says so; U4's fixture confirms it once.
- `temporalio-client/vendored-protox` covers every crate in the tree that
  would otherwise run `protoc`. U2's no-`protoc` build test is where this is
  checked.

---

## Sources

- `docs/plan.md` (the loop, KTD20 exact pins, the replay seam) and
  `docs/plans/2026-11-05-001-feat-managed-agents-sessions-plan.md`
  (sessions, budget, reconnect, deny when unattended).
- `docs/plans/2026-11-07-001-feat-request-event-log-plan.md`, R6: no text in
  a store that outlives the turn.
- `rust/src/agent.rs`, `rust/src/transport/sessions.rs` (the `Sessions`
  trait and `ASK_CLAUDE_REPLAY`), `rust/src/history.rs`, `rust/src/openai.rs`
  (text only, which is why the OpenAI runner is deferred).
- dotfiles `effver` branch, `tasks/README.md`: the HUID and `TASK.md` format.
- `temporalio-sdk` 1.2.0 README: `#[workflow]`, `#[update]`, `wait_condition`,
  `timer`, `workflows::select!`, the nondeterminism detector, `patched`,
  `WorkflowReplayer`, the `testing` feature; its `Cargo.toml` features.
- `temporalio-protos` 0.11.0 `build.rs`: `protoc` by default,
  `vendored-protox` optional.
- Temporal Replay 2027 workshops (replay.temporal.io/tickets/sf27): "Agentic
  SDLC: Durable Pipelines for When AI Writes the Code"; "Workflow Versioning
  + Event History".
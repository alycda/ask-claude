# ask-claude: import claude.sh, then port to Rust

Status legend: `[ ]` not started, `[t]` tests written (red), `[x]` green.
Owner: **A** = Alyssa writes the code, **C** = Claude writes tests/fixtures.

## Decisions (settled 2026-11-02)

| Decision | Choice | Consequence |
| --- | --- | --- |
| Streaming | SSE only, imported from claude.sh as-is | the reassembler is the core; no non-streaming path |
| API key | `ANTHROPIC_API_KEY` only | no `~/.config/anthropic/api_key` fallback; `.env` + `just` dotenv in the container |
| CLI shape | usage spec, subcommands `list` and `resume [chat]` | completion of chat names comes from `usage_cmd` |
| Test seam | `ASK_CLAUDE_REPLAY=<file.sse>` replaces curl | every behaviour test runs offline against recorded fixtures |
| Request log | `ASK_CLAUDE_REQUEST_LOG=<file>` appends each request body | lets replay tests assert what would have been sent (container id reuse) |
| Beta log | `ASK_CLAUDE_BETA_LOG=<file>` appends each request's `anthropic-beta` value | the headers never reach the request log; lets replay tests assert which betas a turn sends (the advisor beta on resume) |
| State dir | `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude/` | tests point `XDG_STATE_HOME` at a temp dir |
| Providers | contract tests now, implementation in Rust | OpenAI-style SSE is a second reassembler; not worth a bash twin |
| History shape | merges for independent lines, chains for dependencies | see "Graph" |

## Known bugs carried from claude.sh

| Bug | Spec for the fix | Where |
| --- | --- | --- |
| A multi-line paste in the chat loop sends on the first newline | one paste is one user turn | T7 precondition test + F7; under bash >= 5.1 `read -e` already honours bracketed paste, so the fix is the interpreter, not the loop: the container has 5.2, macOS /bin/bash 3.2 does not, so on macOS this waits for the Rust port (reedline) |

## Oracle

claude.sh was the reference implementation. Its three inline jq programs were
extracted verbatim and run over recorded SSE fixtures to produce the goldens in
`tests/fixtures/`. A golden is not a design choice; it is "what claude.sh did".
Change a golden only with a commit message saying why that behaviour was wrong.

claude.sh itself was removed after 0.1.4, once the bash `ask-claude` did
everything it did. It is still in the history: as imported, in "feat: import
claude.sh as originally written", and as last kept, with the security-audit
fixes, at the `v0.1.4` bookmark (`jj file show -r v0.1.4 docs/claude.sh`).

Fixtures (recorded 2026-11-02 against api.anthropic.com, ids redacted):

| File | What it exercises |
| --- | --- |
| `sse/text.sse` | one text block, `end_turn` |
| `sse/thinking.sse` | summarized thinking block + text, signature delta |
| `sse/code-execution.sse` | `server_tool_use` + `bash_code_execution_tool_result` + text, container id |
| `sse/error.sse` | HTTP 404, non-SSE JSON error body |

Missing on purpose: a `pause_turn` fixture (cannot be forced) and a `refusal`
fixture (should not be forced). Both paths are covered by hand-written
fixtures derived from `text.sse` with the `stop_reason` edited.

## Graph

```file
└─ P   docs: plan                                         (this file)
   └─ T0  test: harness + fixtures + goldens              C
      ├─ T1  test: reassembler        → F1 feat: lib/reassemble.jq   A
      ├─ T2  test: stream printer     → F2 feat: lib/stream.jq       A
      ├─ T3  test: request builder    → F3 feat: lib/request.jq      A
      ├─ T4  test: usage spec         → F4 feat: subcommands         A
      ├─ M1  merge: phase 1 (F1..F4)                                 A
      └─ T5..T9 chain (rebase onto M1 when it exists)                C
           T5  test: one-shot via replay + live hurl → F5            A
           T6  test: history, list, resume          → F6            A
           T7  test: chat loop                      → F7            A
           T8  test: skills + container             → F8            A
           T9  test: provider contracts (hurl)      → F9 spec flag  A
```

Rule: a `feat` commit is a child of its `test` commit. The test commit is red;
the feat commit turns it green with no edit to the test. If the test has to
change, that is its own commit with the reason in the message.

## Phase 1: pure functions (no network)

| # | Commit | Files | Validation | Status |
| --- | --- | --- | --- | --- |
| T0 | test: harness | `mise.toml` (+bats), `justfile` (`test-unit`, `test-live`), `tests/fixtures/**`, `tests/unit/helpers.bash` | `just test-unit` runs, 0 tests | [t] |
| T1 | test: reassembler goldens | `tests/unit/reassemble.bats` | `jq -s -f lib/reassemble.jq fixture` == `fixture.message.json`, 3 fixtures | [x] |
| F1 | feat: lib/reassemble.jq | the `jq -s` program from `call()`, verbatim | T1 green | [x] |
| T2 | test: stream printer goldens | `tests/unit/stream.bats` | `jq -rj -f lib/stream.jq` == `fixture.stream.txt` | [x] |
| F2 | feat: lib/stream.jq | the `--unbuffered -rj` filter, verbatim | T2 green | [x] |
| T3 | test: request builder goldens | `tests/unit/request.bats` | 4 arg sets == `request/*.json`; `skills_json` == `skills.json` | [x] |
| F3 | feat: lib/request.jq + lib/skills.jq | the `jq -n` body and the `$SKILLS` parser | T3 green | [x] |
| T4 | test: usage spec | `tests/unit/spec.bats` | `--help` lists `list`/`resume`; `usage explain` binds every flag; `resume [chat]` optional | [x] |
| F4 | feat: subcommands in the spec | `ask-claude` `#USAGE` block only | T4 green, `just lint` | [x] |

jq file contract: each `lib/*.jq` is a complete program (`jq -f`), takes its
inputs exactly the way claude.sh passed them (`--arg m`, `--argjson max`,
`--arg sys`, `--arg eff`, `--arg skills`, `--arg cid`, `--slurpfile msgs`),
so the goldens stay valid and the Rust port has one shape to match.

## Phase 2: the live path

| # | Commit | Validation | Status |
| --- | --- | --- | --- |
| T5 | test: one-shot | replay `text.sse`: stdout is `pong`, stderr has the `[model in= ... ]` usage line, history gets 2 turns; replay `error.sse`: exit 1, user turn removed; `tests/anthropic-stream.hurl` live: `message_start` and `message_stop` present | [t] |
| F5 | feat: one-shot | curl \| tee \| printer, then reassembler; `ASK_CLAUDE_REPLAY` short-circuits curl | T5 green, `just test-live` |
| T6 | test: history | `list` empty → no output, exit 0; `list` with 2 files → newest first, turn count, first-prompt excerpt; `resume` unknown → exit 2; `resume <prefix>` picks the match; empty histories skipped | [t] |
| F6 | feat: history files | one JSON array per chat under the state dir | T6 green |
| T7 | test: chat loop | stdin `/new`, `/history`, `/quit`; empty chat file removed on quit; `resume with:` hint printed | [t] |
| F7 | feat: chat loop | `read -e -p "> "` loop from claude.sh | T7 green |
| T8 | test: skills | request golden with `-s pptx,skill_x@skver_1`; replay `code-execution.sse` writes `$HIST.container`; next request carries `container.id` | [t] |
| F8 | feat: skills + container persistence | | T8 green |

## Phase 3: providers (contracts only)

| # | Commit | Validation | Status |
| --- | --- | --- | --- |
| T9 | test: provider contracts | `tests/providers/openai-compatible.hurl`, run by `just test-openai` with `base_url`, `api_key`, `model` from `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL`: `POST /chat/completions`, `stream: true`, `chat.completion.chunk`, `finish_reason: stop`, `data: [DONE]`; one provider per run (Venice is a base URL, not a provider), never run by `just test` | [t] |
| F9 | feat: `--provider` in the spec | `flag "-p --provider <provider>" default="anthropic"`; bash rejects anything else with exit 2 and the message "not implemented in the bash version, see docs/plan.md" | T9 unchanged, spec test extended |

Venice is OpenAI-compatible at `https://api.venice.ai/api/v1` (Bearer auth;
accepts `max_completion_tokens` and `stream_options`; `venice_parameters` is
its only extension, `store` and `user` are accepted and ignored). OpenAI chat
streaming: `data:` lines carrying `chat.completion.chunk` objects, terminated
by `data: [DONE]`. One reassembler covers both; it is written once, in Rust.

| `-p openai` against | `OPENAI_BASE_URL` | `OPENAI_API_KEY` | `OPENAI_MODEL` |
|---|---|---|---|
| OpenAI | `https://api.openai.com/v1` | an OpenAI key | `gpt-4.1-mini` |
| Venice | `https://api.venice.ai/api/v1` | a Venice key | `gemini-4-argon` |

## Phase 3b: sessions on Managed Agents (0.2.x)

Planned in `docs/plans/2026-11-05-001-feat-managed-agents-sessions-plan.md`;
the units there are the commits here. 0.2.0: U1 fixtures and goldens, U2
`lib/session.jq` and `lib/session-fold.jq`, U3 the `agent` command in the
spec, U4 one shot, U5 chat, resume, list, U6 tool confirmations, U7 live
contracts (`just test-agents`) and recipes. 0.2.1: U8 files. 0.2.2: U9
vaults, repository, memory. 0.2.3: U10 session operations. The one-stream-
per-turn seam (`ASK_CLAUDE_REPLAY` as a directory of turns) is the bats
seam for all of it, and the Rust `Provider` trait gains a `Sessions`
transport at R7.

## Phase 4: Rust port

Crate `ask-claude` in `rust/` (own line, merged when it reaches parity). The
scaffold, the tests and the benchmarks exist (2026-11-05); Alyssa writes the
Rust, one unit per commit, each turning its `r<N>_*` tests green: `just
rust-test`, `just rust-lint`, `just parity`, then `just test-rust` from R5.

| # | Commit | Validation | Status |
| --- | --- | --- | --- |
| R0 | chore: cargo scaffold, clap + usage-lib 7 | `rust/`: `src/cli.rs` mirrors the `#USAGE` block, `--usage-spec` prints it through `usage-lib` 7.3.0, the version of the `usage` CLI (env and value arity read back from clap); `just parity` runs `usage diff ask-claude <(ask-claude --usage-spec)`: 0 breaking. `src/lib.rs` is the shape with `todo!()` bodies; `rust/tests/goldens.rs` and `rust/tests/cli.rs` the red tests; `rust/benches/goldens.rs` the criterion benchmarks | [x] |
| R1 | feat: SSE event types (serde) | `sse::parse` on every data line of every `sse/*.sse` and `sessions/**.sse` fixture, `kind()` equal to the JSON type (`r1_*`; green with the placeholder `Event`, which the port may make an enum) | [x] |
| R2 | feat: reassembler | `reassemble::message(events) == *.message.json` as `serde_json::Value`, tool inputs parsed and `partial` gone (`r2_*`) | [x] |
| R3 | feat: request builder | `request::body(args) == request/*.json` for plain, system-effort, skills, container, web, mcp, everything; `request::skills` (`r3_*`) | [x] |
| R4 | feat: stream printer | `stream::line` concatenated == `*.stream.txt` bytes; `stream::safe` (`r4_*`) | [x] |
| R5 | feat: transport trait, replay impl | `transport::Transport` with `ASK_CLAUDE_REPLAY` (a file, or a directory of turns) and `ASK_CLAUDE_REQUEST_LOG`; `r5_one_shot_through_replay_prints_pong`, then `just test-rust` (the bats suite with `ASK_CLAUDE_BIN` at the Rust binary) | [x] |
| R6 | feat: history, list, resume, chat loop, sessions, event log lines | the same bats files, same `XDG_STATE_HOME` seam; `session::{line, fold, create_body, history}` against the U1 goldens (`r6_*`); the event log (`docs/plans/2026-11-07-001-feat-request-event-log-plan.md`) is `tests/unit/events.bats`, the same line shape as `lib/event.jq`'s goldens | [x] |
| R7 | feat: `Provider` trait: `Anthropic`, `OpenAiCompatible { base_url }`, and the `Sessions` transport (one stream per turn, events sent after the stream opens) | `tests/providers/openai-compatible.hurl` green; `tests/agents/*.hurl` green; `--web` and `--mcp` through `request::body` | [x] |
| R8 | chore: `just test-rust` green, `just rust-bench` recorded | both binaries green on the whole suite; `just parity` 0 breaking; the three live hurl sets green; bash stays the oracle until R8 is merged | [x] |

Parity gate before replacing the bash binary: every bats test green on both,
`usage diff` empty, the three live hurl files green.

### The phase 4 gate

R8 is the last row of phase 4. Its validation was met on 2026-11-14 and is
recorded in `verified(1.0.0): the gate passed on rust-parity@ 2026-11-14`:
both binaries green on the whole suite, `just parity` 0 breaking, the three
live hurl sets green. The row is checked for that. That gate built the
benchmarks (`cargo bench --no-run`) and recorded no timing, so steps 1 to 5
below are a rerun and step 6 is new. The gate, in order (the row above says
`just rust-bench` and `just rust-lint`; the recipes are `just bench` and
`just lint`):

1. `just lint-bash`, `just lint`, `just actionlint`
2. `just unit-tests`, `just pty-test`: the script
3. `just rust-test`, `just test-rust`: the Rust binary on the same suite
4. `just parity`: 0 breaking
5. `just test`, `just test-openai`, `just test-agents`: live, with keys
6. `just bench`, `just bench-compare`: recorded in the verified commit's
   body, `rust/target/bench-compare/summary.md` pasted whole

`just bench-compare` times the script against the Rust binary, so this gate
is the last place that comparison can be made: phase 5 removes one side of
it. "The Rust binary is faster" is a claim only once this table exists.

## Phase 5: the Rust binary alone

Planned 2026-11-18. The script needs bash 4.4 for `--mcp` (`${v@Q}`,
`${h^^}` in `lib/config.bash`) and bash 5.1 for a paste in the chat loop;
the machine it was written for has `/bin/bash` 3.2 and runs the Rust binary.
Phase 5 removes the script the way claude.sh was removed after 0.1.4. It
starts from the verified phase 4 gate and changes no behaviour of the Rust
binary: `usage diff` between the last script and the first snapshot is empty.

### Decisions (proposed 2026-11-18, not settled)

| Decision | Choice | Consequence, and the alternative not taken |
| --- | --- | --- |
| Oracle | the goldens, frozen, and the live contracts | `lib/*.jq` goes with the script. A new fixture's golden is the Rust binary's output, read line by line in the commit that adds it. Not taken: `lib/*.jq` kept under `tests/` as a test-only oracle, which keeps a second implementation to write for every feature |
| Spec | clap is the source; `ask-claude.usage.kdl` at the root is its tracked snapshot | the `#USAGE` block was both source and record. `just effort-floor` and review need a spec in the tree at every version commit, without building an old binary. Not taken: `effort-floor` builds the last version's binary, which needs a toolchain and minutes for a diff |
| Black-box suite | bats stays, run against the Rust binary | about 260 of the 364 tests drive a binary through the replay seam and do not care what it is written in. The developer's machine still needs bash for bats, the justfile's shebang recipes, `tools/effver/` and `tests/record-session.sh`; a machine that only runs the binary needs none |
| jq-program tests | deleted, after each assertion has a twin in `rust/tests/goldens.rs` | `reassemble`, `stream`, `request`, `session`, `event` and the three `*-steps` files (92 tests) run `jq -f lib/*.jq` and test nothing once it is gone; so does one test in `agent-chat.bats`, over `lib/session-history.jq` |
| Crate location | `rust/` stays | moving it to the root rewrites every path in CI, the justfile and the docs for no change an adopter sees |
| Recovery | a bookmark at the last version that ships the script | `jj file show -r <bookmark> ask-claude`, as `v0.1.4` keeps claude.sh |

### Graph

```file
└─ P5  docs: plan phases 5 and 6                           (this section)
   └─ R8  chore: the parity gate recorded → V4 verifier    A
      ├─ D1  test: the spec snapshot  → D2 feat: effort-floor reads it   C, A
      ├─ D3  test: Rust twins of what the script's tests pin             C
      ├─ M5  merge: the spec line and the twins line                     A
      └─ D4  test: the suite runs the Rust binary                        C
         └─ D5  chore: remove the script and lib/                        A
            └─ D6  docs: one binary                                      C
               └─ V5 verifier → version                                  A
```

D1 and D3 are independent: one adds a file the tools read, the other adds
Rust tests. D4 needs both, because it deletes the tests D3 twins and the
`parity` recipe D2 replaces. D5 needs D4: until the suite no longer defaults
to the script, removing it turns every test red at once.

| # | Commit | Files | Validation | Owner | Status |
| --- | --- | --- | --- | --- | --- |
| D1 | test: the spec snapshot | `ask-claude.usage.kdl` (the output of `ask-claude --usage-spec`, the golden); `rust/tests/cli.rs`; `tests/unit/effort-floor.bats` | red until D2 for exactly three tests in `effort-floor.bats`: the floor with the snapshot at both commits; with the script at the last version and the snapshot at the tip (the one release that spans the removal); with both in one commit, where the snapshot is the one read. Green on arrival, and red on the parent only because the file is not there: `--usage-spec` equals the snapshot byte for byte. Green before and after: with neither, "nothing to compare" | C | [x] |
| D2 | feat: `effort-floor` and `check-spec` read the snapshot | `tools/effver/effver.just`, `justfile` | D1 green, no edit to it. `just spec` rewrites the snapshot; `just check-spec` fails when it is stale and replaces `just parity` in `rust.yml`. `effort-floor` reads `ask-claude.usage.kdl` at the last version commit and falls back to `ask-claude` when that commit has no snapshot. `just parity` stays as a recipe until D5 | A | [x] |
| D3 | test: Rust twins of what the script's tests pin | `rust/tests/goldens.rs`, `rust/tests/cli.rs` | green on arrival: it moves assertions, it adds no behaviour. Each assertion in the eight jq-program bats files, and the `session-history.jq` test in `agent-chat.bats`, is matched to a test in `goldens.rs` in the commit body, or ported. Each of the 13 `bash-only` tests gets the disposition below. A test that turns out red is a parity bug and gets its own test-then-feat pair before M5 | C | [ ] |
| D4 | test: the suite runs the Rust binary | `tests/unit/helpers.bash`, `tests/unit/*.bats`, `justfile` | `BIN` defaults to `rust/target/debug/ask-claude`; `unit-tests` builds first; `test-rust`, `pty-test`, `step` and the `bash-only` and `rust-only` tags are gone; `spec_file` has one branch; the eight jq-program files, the `session-history.jq` test and the `bash-only` tests are deleted. `just unit-tests` green with the script still in the tree and unread: `chmod -x ask-claude` changes no result | C | [ ] |
| D5 | chore: remove the script and `lib/` | `ask-claude`, `lib/`, `justfile` (`_help`, `_agents-help`, `ask-bash`, `bench-compare`, `parity`, `check-version-line`, the `sed` in `bump`), `mise.toml` and `mise.lock` (hyperfine), `.github/workflows/` (`bash.yml` becomes the bats job on the Rust binary; `rust.yml` loses the `ask-claude` path and `continue-on-error`; the `spec` job in `lint.yml` reads the snapshot; the `entity-diff.yml` comment) | `git grep -n 'lib/.*\.bash\|\./ask-claude'` finds no recipe or workflow; `just lint-bash` still covers bats, `tools/` and `tests/record-session.sh`; `just unit-tests`, `just rust-test`, `just check-spec`, `just actionlint` green | A | [ ] |
| D6 | docs: one binary | `README.md` (install, no runtime dependencies, the bash 5.1 line gone), `AGENTS.md`, `CONCEPTS.md` (Oracle, Unit suite, Usage spec, Parity gate), `CODING_STANDARDS.md` (the jq-program rules and the bash-arithmetic rule retired, each with the commit that retired it), this file's Oracle section | no document names a file D5 removed, except as history at the bookmark. Doc comments in `rust/src` that cite `lib/*.jq` stay: they name the oracle a function was matched against | C | [ ] |

The 13 `bash-only` tests:

| Test | Disposition |
| --- | --- |
| `agent-confirm`: an always answer left by an earlier process with this PID | deleted: the Rust binary keeps the grant in memory (`session.rs`), so there is no file to plant |
| `chat`: bash >= 5.1, twice | deleted: `terminal_one_paste_is_one_turn_and_ctrl_d_quits` in `rust/tests/terminal.rs` tests the paste itself |
| `refuse` and `provider`: `-p openai` refused by the bash version | deleted: the Rust binary implements it, under the `rust-only` tests that lose their tag |
| `layout`: both | deleted with `lib/` |
| `agent`: the token never goes on a command line | deleted; the rule stays in `CODING_STANDARDS.md`, and its surface in Rust is the one `Command::new` in `local_mcp.rs` |
| `argv`: both | the same |
| `spec`: the script's copy of the version | replaced: `--version` prints `VERSION` (D3 checks `rust/tests/cli.rs` has it) |
| `provider`: the recording script reads `ANTHROPIC_BASE_URL` | kept, tag dropped, `ask-claude` and `lib` out of its grep |
| `state-dir`: cannot be made private; another account owns it | deleted: `history.rs` tests both messages without a shim |

Gate (V5), in order: `just lint-bash`, `just lint`, `just actionlint`,
`just rust-test`, `just unit-tests`, `just check-spec`, `just test`, `just
test-openai`, `just test-agents`; then, on the machine with `/bin/bash` 3.2,
a release build, one shot, `--mcp` with a query-string URL, and a two-line
paste in the chat loop that is one turn. The last step is the reason for the
phase, so it is in the gate and not left to the release notes.

Version: `usage diff` is empty, so `just effort-floor` says micro. The floor
does not see that adopting this release means building a binary where a
script was copied; the label is the maintainer's.

## Phase 6: colour

Planned 2026-11-18, after phase 5: with one binary there is one printer to
colour and no second set of goldens to keep in step. No TUI: every role
below is a property of a line the binary already prints.

### Decisions (proposed 2026-11-18, not settled)

| Decision | Choice | Consequence, and the alternative not taken |
| --- | --- | --- |
| Flag | `--color <when>`: `auto` (the default), `always`, `never`; global | a spec change: the snapshot moves in the commit that adds it, and an added optional flag keeps the floor at micro |
| `auto` | decided per stream: on when that stream is a terminal, `NO_COLOR` is unset or empty, and `TERM` is not `dumb` | `ask-claude q \| less` keeps the usage line on stderr coloured and stdout plain. `always` and `never` beat the environment. Not taken: `CLICOLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR`, which the flag already covers |
| Existing goldens | none changes | under bats no stream is a terminal, so `auto` is off and every `*.stream.txt` holds byte for byte. Colour has its own goldens, `*.stream.color.txt`, made under `--color=always` |
| Never coloured | the reply's text; history files; event log lines; the request log; a value shown for approval | the reply is the output, and the rest is read by programs or must be shown exactly as encoded (`CODING_STANDARDS.md`, "What reaches the terminal") |
| Sequences | dim, bold, red, yellow and reset, as five constants | the terminal's theme decides contrast. Not taken: 256-colour or truecolor, and a styling crate; `nu-ansi-term` is already in the tree under reedline, but five constants need no dependency and no tree review |
| Spans | a span opens and closes inside one write | no interrupt, error or `SIGPIPE` can leave the terminal coloured, so there is no signal handler to write. SGR is added after `safe`, which is unchanged: an escape in API-controlled text is still removed under `--color=always` |

Roles:

| Line | Stream | Style |
| --- | --- | --- |
| reply text | stdout | none |
| `(thinking) `, `(web_search) `, `(web_fetch) `, `(running code) `, `(mcp <server>) `, `(advisor) `, `(result rc=0) ` | stdout | dim |
| `(result rc=<n>) ` with n not 0, `(advisor error: <code>) ` | stdout | red |
| `[<model> in=… out=… <s>s]`, the agent usage line | stderr | dim |
| `[stopped: …]`, `[pause_turn: …]` | stderr | yellow |
| `ask-claude:` in an error line | stderr | bold red, the prefix only: the message stays at full contrast |
| the chat prompt, the tool name in a confirmation prompt | terminal | bold |

### Graph

```file
└─ V5  the phase 5 gate, verified
   └─ K1  test: --color in the spec     → K2 feat: the flag            C, A
      └─ K3  test: when colour is on    → K4 feat: color::enabled      C, A
         ├─ K5  test: stdout markers    → K6 feat: the stream printer  C, A
         ├─ K7  test: stderr lines      → K8 feat: the stderr lines    C, A
         ├─ K9  test: prompts           → K10 feat: prompts            C, A
         └─ M6  merge: the three printers → V6 verifier → version      A
```

K3 needs the flag K2 parses. K5, K7 and K9 are independent of each other:
each colours a different writer and reads only `color::enabled`.

| # | Commit | Files | Validation | Owner | Status |
| --- | --- | --- | --- | --- | --- |
| K1 | test: `--color` in the spec | `ask-claude.usage.kdl`, `tests/unit/spec.bats` | red for exactly: the snapshot test (the binary has no such flag); `usage` binds `--color` with choices `auto always never`; `--color sometimes` exits 2 before any request | C | [ ] |
| K2 | feat: the `--color` flag | `rust/src/cli.rs` | K1 green; the flag is parsed and read by nothing yet, so every other test is unchanged | A | [ ] |
| K3 | test: when colour is on | `rust/src/color.rs` (the signature, `todo!()` body), `rust/tests/color.rs` | red for exactly the truth table of `color::enabled(when, is_terminal, no_color, term)`: 3 x 2 x 3 x 2 rows, `NO_COLOR` unset, empty and set, `TERM` `dumb` and not | C | [ ] |
| K4 | feat: `color::enabled` | `rust/src/color.rs`, `rust/src/main.rs` | K3 green; one decision per stream, made once at startup | A | [ ] |
| K5 | test: stdout markers | `tests/fixtures/sse/*.stream.color.txt`, `rust/tests/goldens.rs`, `tests/unit/stream.bats` or its successor | red for exactly: each fixture's printer output under colour equals its colour golden; stripping SGR from a colour golden gives the plain golden; an `ESC[31m` inside a text delta is removed under `--color=always`; `--color=always` into a pipe emits SGR and `--color=never` emits none | C | [ ] |
| K6 | feat: the stream printer colours markers | `rust/src/stream.rs` | K5 green; every `*.stream.txt` test unchanged | A | [ ] |
| K7 | test: stderr lines | `tests/unit/oneshot.bats`, `tests/unit/agent.bats` | red for exactly: under `--color=always` the usage line is dim, `[stopped: …]` yellow, the `ask-claude:` prefix bold red, each closed by a reset before its newline; with stdout a pipe and stderr forced on, stdout carries no SGR | C | [ ] |
| K8 | feat: the stderr lines | `rust/src/lib.rs`, `rust/src/agent.rs` | K7 green | A | [ ] |
| K9 | test: prompts | `rust/tests/terminal.rs` | red for exactly: on a pty the chat prompt is bold and `NO_COLOR=1` makes it plain; the confirmation prompt's tool name is bold and the value shown for approval is byte for byte what it was | C | [ ] |
| K10 | feat: prompts | `rust/src/chat.rs`, `rust/src/agent.rs` | K9 green | A | [ ] |

The colour goldens have no oracle to copy. Each is the plain golden with the
role table applied by hand, and K5's commit says so; the role table is what
a later change to a colour golden is argued against.

Gate (V6), in order: `just lint`, `just rust-test`, `just unit-tests`, `just
check-spec`, `just test`; then by eye on a dark and a light terminal theme,
and on the machine phase 5 was for: a reply with thinking, one with a failed
code execution, an error, a chat, `--color=never`, and `NO_COLOR=1`.

Version: one optional flag, so `just effort-floor` says micro. Colour on by
default on a terminal changes what a person sees and nothing a program
reads; the label is the maintainer's.

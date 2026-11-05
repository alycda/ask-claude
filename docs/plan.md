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
| State dir | `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude/` | tests point `XDG_STATE_HOME` at a temp dir |
| Providers | contract tests now, implementation in Rust | OpenAI-style SSE is a second reassembler; not worth a bash twin |
| History shape | merges for independent lines, chains for dependencies | see "Graph" |

## Known bugs carried from claude.sh

| Bug | Spec for the fix | Where |
| --- | --- | --- |
| A multi-line paste in the chat loop sends on the first newline | one paste is one user turn | T7 precondition test + F7; under bash >= 5.1 `read -e` already honours bracketed paste, so the fix is the interpreter, not the loop: the container has 5.2, macOS /bin/bash 3.2 does not, so on macOS this waits for the Rust port (reedline) |

## Oracle

claude.sh is the reference implementation. Its three inline jq programs were
extracted verbatim and run over recorded SSE fixtures to produce the goldens in
`tests/fixtures/`. A golden is not a design choice; it is "what claude.sh does
today". Change a golden only with a commit message saying why claude.sh was
wrong.

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

## Phase 4: Rust port

Crate `ask-claude` in `rust/` (own line, merged when it reaches parity).

| # | Commit | Validation | Status |
| --- | --- | --- | --- |
| R0 | chore: cargo scaffold, clap + clap_usage 5 | `cargo run -- --usage-spec` prints a spec; `usage diff ask-claude.usage.kdl <(cargo run -q -- --usage-spec)` reports no interface change (the bash spec is exported with `usage generate` first) | [ ] |
| R1 | feat: SSE event types (serde) | deserialize every line of every `sse/*.sse` fixture without `#[serde(other)]` swallowing anything | [ ] |
| R2 | feat: reassembler | `Message::from_events(fixture) == fixture.message.json` (serde_json::Value equality) | [ ] |
| R3 | feat: request builder | `Request::new(args).to_json() == request/*.json` | [ ] |
| R4 | feat: stream printer | same bytes as `*.stream.txt` | [ ] |
| R5 | feat: transport trait, replay impl | the bats suite runs unchanged against the Rust binary via `ASK_CLAUDE_BIN` | [ ] |
| R6 | feat: history, list, resume, chat loop | same bats files, same `XDG_STATE_HOME` seam | [ ] |
| R7 | feat: `Provider` trait with two impls: `Anthropic` and `OpenAiCompatible { base_url }`; openai and venice are rows in a table, not types | `tests/providers/openai-compatible.hurl` green for both rows; a recorded OpenAI-shape fixture gets its own goldens then | [ ] |
| R8 | chore: `just test-rust` runs the bats suite against both binaries | both green; bash stays the oracle until R8 is merged | [ ] |

Parity gate before replacing the bash binary: every bats test green on both,
`usage diff` empty, the three live hurl files green.

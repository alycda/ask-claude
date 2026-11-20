# Security audit report: ask-claude Rust port (run 3)

## 1. Run profile, scope, execution and prior-run use

**This is a partial pass of the repository.** It was a `standard`-profile run scoped to `rust/` only, the Rust port of `ask-claude`. The bash script `ask-claude`, `lib/*.jq` and `lib/*.bash` were read only as the oracle that the port mirrors. Every surface outside `rust/` was recorded `out_of_scope` (listed below). The repository as a whole has not been audited by this run.

| Item | Value |
|---|---|
| Profile | `standard` |
| Scope | `rust/` (scoped run, partial pass) |
| Budget | None set. 68 agent invocations; with no budget, there is no planned figure to compare against. |
| Agent invocations | 4 reconnaissance, 1 architecture/ledger synthesis, 21 hunters over 5 waves plus 1 same-owner reissue of a malformed hunter result, 5 post-wave critics, 1 final-clean critic, 17 Phase 3 verifiers, 17 Phase 5 verifiers, 1 fresh verifier for a material Phase 5 replacement |
| Source ref | commit `71ae96303e6f4fc755773a3d491adb86d308ec9b` ("fix(test): history.bats declares the quoted backticks to shellcheck"). The worktree was clean, and the source files were read-only for the run. |
| Run status | `complete` (phases 1 to 5 done; this is phase 6) |

**Execution policy: sandboxed source-and-local-only. No target code was executed.** The policy allows bounded local checks only inside an OS-enforced sandbox: no network, an empty allowlisted environment, a read-only target, scratch-only writes, and resource and wall-clock limits. No such sandbox was available on the audit host: `unshare` was refused, and there was no bwrap, firejail or nsjail.

So the crate was never built. Its proc macros and the `ring` build script count as target-controlled code. No binary, unit test or function harness was run, and no runtime artifact was promoted. Every check in this run is a source check. Wherever the decisive fact was a runtime observation, the record became `needs_validation`.

**This is why no record reached `confirmed`.** Confirmation requires a bounded local observation, and the run could not make one. It says nothing about the quality of the leads. Each of the 17 records has a complete, independently verified source trace and an exact local plan, ready to run once a sandbox exists.

**Prior-run use.** Run 2 (source `9c21790e527c3f8ca3b5f569d26f3a14e8f8c664`; 28 ledger units, 8 findings, both validators passing) was read and found compatible. All of its records target the bash script and jq programs, which are out of this run's scope. No prior ledger covered `rust/`, so every unit here is `new`. **No same-source confirmations are carried**: run 2's confirmed records are against other sources, and none is re-reported or used as an exemplar. Where run 2 assigned a severity to a bash twin, the Rust record cites it as calibration only. It is not carried as a severity here.

Six run-2 records have a Rust twin with the same root cause. Each became a `needs_validation` lead in this run:

| Run-2 record (bash/jq) | Rust lead in this run |
|---|---|
| state-dir owner/parent check | Lead 8: `rust/src/history.rs:private_state:ancestor-unchecked-by-name-opens` |
| custom-round cap skipped when asks are present | Lead 12: `rust/src/agent.rs:Agent::turn:custom-round-cap-skipped-when-asks-present` |
| `lib/request.jq:mcp_token:host-variable-name-collision` | Lead 2: `rust/src/request.rs:token-variable:non-injective-host-fold` |
| MCP user-info host mismatch | Lead 1: `rust/src/lib.rs:url-host:mcp-userinfo-token-host-mismatch` |
| `lib/request.jq:tools:mcp-toolset-unconfirmed-with-untrusted-retrieval` | Lead 10: `rust/src/request.rs:toolset:default-all-tools-with-untrusted-retrieval` |
| `lib/session-create.jq:memory_resource:unrecognized-access-suffix-fails-open` | Lead 11: `rust/src/session.rs:create_body:memory-access-suffix-fails-open` |

**Changed-source revalidations.** Critics w1, w2 and w3 re-checked the Rust twins of the other two run-2 leads against source and found both fixed in Rust:

- `ask-claude:dollars-tonumber-error-bypasses-safe`. `dollars` in `rust/src/agent.rs` returns `None` for a non-number, a non-numeric string or a non-finite value. `list` breaks, `budget` returns 1 silently, and `usage_line` gets `None`. Every printed line goes through `safe`.
- `ask-claude:session_answer:confirm-display-bidi-format-chars-unstripped`. `for_approval` in `rust/src/lib.rs` escapes C1, bidi and invisible characters in the input shown for approval. One residual remained: the tool name goes through `safe` only. Hunter h1-session-turn found that residual unreachable.

Lead 7 (invisible format and tag code points that `for_approval` does not escape) is a separate root cause found in this run. It is not a reopening of the bidi lead.

**Coverage outside the run.**
- **Deferred units: 0.**
- **Blocked units: 1.** The RustSec known-CVE check of `rust/Cargo.lock`; see part 7.
- **Out-of-scope units: 19**, all because of the `rust/` scope:
  - **Toolchain and CI:**
    - the devcontainer and mise toolchain install
    - Rust CI in `.github/` (build, parity, fmt, clippy and tests)
  - **`justfile` recipes:**
    - the Rust recipes
    - the general recipes, `.env` and recipe arguments
  - **The shipped bash script:**
    - `agent tools` metadata printed by jq
    - `ASK_CLAUDE_REPLAY` in the script
    - curl to the API root under proxy variables
    - `--mcp`/`--mcp-tool` values containing tab or newline
    - assistant content blocks
  - **Bash twins of Rust units:**
    - the confirmation prompt on stderr with the answer read from `/dev/tty` (`lib/agent-turn.bash`)
    - the empty-`stop_reason` success fallback (`lib/agent-turn.bash`)
    - the `FOR_APPROVAL` escape set (`lib/common.bash`)
    - duplicate event ids kept by `lib/session-fold.jq` and the answer loop
  - **Tests and tools outside `rust/`:**
    - the bats harness that runs the Rust binary
    - `VERSION` and the date argv in `tools/`
    - the effver bump recipe
  - **Two repository-wide "all repository files" units**
  - **The audit sandbox wrapper `tools/security-audit/sandbox`**, which is audit tooling and not part of the product

**Verification.** Each of the 17 records went to a fresh Phase 3 source verifier and then to a fresh Phase 5 record verifier.
- Phase 5 left 11 records unchanged.
- Five got non-material corrections: a budget-default evidence entry, a README line number, two validation-plan fixes, and one removed non-decisive blocker.
- One (Lead 4) got a material trace correction, and a further fresh verifier re-verified it.
- No verdict changed. All 17 remain `needs_validation`.

## 2. Security posture summary

`rust/` is a single-user CLI with no listener. Its main defences hold in source:
- `base_url_gate` allows only https or exact-loopback http, refuses any `@`, and redirects are off.
- Ids are shape-checked before they name files or URL paths.
- Downloads use `create_new` 0600 plus rename.
- `safe` filters terminal output, and `for_approval` escapes the approval input.
- The request log has an owner/mode check.
- Local MCP calls are sent only as JSON values to an allowlist of names the server itself lists.

The open questions sit at four edges:
- whether an MCP token is bound to exactly one host (Leads 1 and 2)
- the cleartext loopback exception under an environment proxy (Lead 3)
- whether a terminal approval is bound to the action that was shown (Leads 4 to 7)
- fail-open handling of restrictions the user asked for (Leads 9 to 11)

Truncated or framing-confused streams are reported as success (Leads 13 to 15). None of these is confirmed. Each needs a sandboxed local run, and most also need one provider, terminal or deployment fact.

## 3. Confirmed findings

| Severity | Title | Affected boundary | Observed result |
|---|---|---|---|
| none | No confirmed findings | none | none |

`findings.json` holds 17 final records, all `needs_validation`, with none `confirmed` and none `rejected`. Confirmation needs a bounded local observation of the boundary result. This run could execute no target code, because no OS-enforced sandbox was available (part 1), so no record could be confirmed. No severity is assigned to any record.

## 4. Confirmed finding details

None. See part 5 and `NEEDS-VALIDATION.md`.

## 5. NEEDS VALIDATION

These are leads, not confirmed vulnerabilities, and none has a severity. **The rows are ordered by the report writer's judgement of which leads most deserve the owner's attention first. That order is not a severity ranking.** It puts credential exposure to another host first, then approval binding at the model-to-local-action boundary, then fail-open handling of restrictions the user chose, and last the integrity of the reported outcome and availability.

**Every row also carries one shared blocker: no OS-enforced sandbox was available, so the target was not built or run.** The blocker column names any further decisive fact. "Sandbox only" means the source fully determines the path, and the only thing missing is the bounded local observation.

`NEEDS-VALIDATION.md` has the verbatim trace, evidence, blockers and plans. Every local step assumes an OS-enforced sandbox with no network, dummy credentials only, and scratch-only writes. Every owner check is for the owner to perform on their own test resources.

| # | Title | Repository trace (short) | Exact blocker (beyond the sandbox) | Bounded local next step | Owner-observed check |
|---|---|---|---|---|---|
| 1 | `--mcp` URL with user info selects one host's token while the URL connects to another host | `rust/src/config.rs:76` check (https prefix only, `@` allowed) → `rust/src/lib.rs:693` `url_host` cut at first `/` or `:` → `rust/src/request.rs:133` `token_variable` → `rust/src/request.rs:59` `body` attaches the token | Whether the Messages API MCP connector (mcp-client-2025-11-20 beta) accepts an https `mcp_servers` URL with user info, and if so connects to the host after `@` and sends `authorization_token` there | Unit test in `rust/src/request.rs`: `body()` with `https://mcp.good.example:x@evil.example/mcp` and only `ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE=dummy-good` yields `authorization_token: dummy-good`, name `mcp-good-example`; `config::check` accepts it; `http::Uri` (http 1.5.0, already locked) reads the host as `evil.example` | One Messages request with `mcp_servers` URL `https://decoy.owner.example:x@listener.owner.example/mcp` (both hosts owner-controlled, dummy token). Record whether the API rejects it, strips the user info, or presents `Authorization: Bearer dummy-token` at the listener |
| 2 | Per-host MCP token variable fold is non-injective, so another host can receive a server's token | `rust/src/config.rs:80` (names used only in an error message) → `rust/src/request.rs:115` `host` folds `.`, `-`, `_` → `rust/src/request.rs:136` `token_variable` → `rust/src/request.rs:59` `body` | Whether the connector presents `authorization_token` as a bearer credential to the server at the entry's URL; for the two-server variant, how the API handles two `mcp_servers` entries with the same name | Scratch unit test: `token_variable` is equal for `mcp.good.example` and `mcp-good.example`, and for two IPv6 literals (`ASK_CLAUDE_MCP_TOKEN__`); `body()` for `mcp-good.example` carries the dummy token; both hosts together pass `config::check` with one token and one name | Two owner header-logging endpoints whose hosts fold to one variable. Set only that variable to a dummy token. Record which endpoint receives the bearer and whether the API rejects the duplicate name |
| 3 | Proxy environment routes the plain-http loopback API request, API key included, through a network proxy | `rust/src/config.rs:234` loopback http allowed → `rust/src/lib.rs:212` → `rust/src/transport.rs:47` `Http::new` keeps ureq's env proxy → `rust/src/transport.rs:84`/`:88` sends `x-api-key` | The decisive library behaviour comes from the checksum-matched cached ureq 3.5.2 crate, which is outside the repository and cannot be cited as a repository line. Exposure also needs a user who runs a plain-http loopback root in a shell exporting `ALL_PROXY`/`HTTPS_PROXY`/`HTTP_PROXY` (or lowercase) without a matching `NO_PROXY` entry | Loopback CONNECT-logging fixture on `127.0.0.1:3128`. Run `--base-url http://localhost:8080` with a dummy key under `HTTP_PROXY`, then `HTTPS_PROXY`, then the `NO_PROXY` variants. The vulnerable result is `CONNECT localhost:8080` followed by a plaintext `x-api-key` | Ask whether users with loopback http gateways run under exported proxy variables, how `NO_PROXY` is spelled (no spaces after commas, never empty), and whether the proxy is remote or a local agent |
| 4 | Tool-confirmation prompt goes to stderr while the approval is read from `/dev/tty`, so with stderr redirected a typed `yes` approves an unseen call | `rust/src/session.rs:17` shows only `(name)` → `rust/src/agent.rs:861` tty check (stdin and `/dev/tty` only) → `rust/src/agent.rs:903` `ask_tty` prints on fd 2 → `rust/src/session.rs:381` `Gate::confirm` | Sandbox only (the fd routing is fully determined by source) | `rust/tests/terminal.rs` harness with `2> scratch/stderr.txt`: replay one `bash` ask, assert no `allow bash` on the pty, type `yes`, assert `results(dir)` is `["allow"]` and the prompt appears only in `stderr.txt`; repeat with `a` | None (the record has no deployment plan) |
| 5 | The agent tool-confirmation prompt takes terminal type-ahead as its answer | `rust/src/agent.rs:1055` `Streamed::take` prints bait live → `rust/src/agent.rs:861` → `rust/src/session.rs:379` `Gate::confirm` → `rust/src/agent.rs:909` `ask_tty` reads without `tcflush` → `rust/src/session.rs:497` call | No trusted pty harness (python3 absent), so the queued line being consumed was not observed. Live-session timing (whether the model can print bait and stay busy before `requires_action`) is not visible in source | Pty pair: write `y\r` before spawning `ask-claude agent go` against a replay with one custom call and a marker-writing MCP stub. Vulnerable if `allowed local probe` and one marker line appear with no key pressed after the prompt. The `tcflush(TCIFLUSH)` fix turns this into a denial | On a real terminal with a marker-only dummy tool, type `yes` during always-allowed server work and record whether the prompt is answered without a key pressed after it (timing only) |
| 6 | In agent chat Pipe mode, a stdin worker thread keeps reading the terminal during turns and competes with `ask_tty` for the confirmation answer | `rust/src/chat.rs:45` Pipe mode → `rust/src/transport.rs:326` free-running reader → `rust/src/agent.rs:909` `ask_tty` on the same terminal → `rust/src/session.rs:379` → `rust/src/agent.rs:642` stray `user.message` | Which of two blocked readers on one terminal gets a canonical-mode line, and whether a line can split after `ask_tty`'s 1-byte read. This depends on Linux `n_tty_read`'s `atomic_read_lock` and wake order, macOS differs, and the repository does not fix the platform or kernel | Drive `<bin> agent 2>&1 \| cat` on the `script` pty harness. Do 50 runs typing `y`, then 50 runs typing `a b` then Enter. Record each confirmation result, any hang, and any stray `user.message` | On Linux and macOS, run `ask-claude agent 2>&1 \| tee /tmp/askc.log` with a harmless tool that needs confirmation. Type `y` at the prompt and record whether it is allowed at once or keeps waiting, and whether a stray `y` message appears |
| 7 | Approval line leaves invisible format and tag code points in tool input unescaped | `rust/src/session.rs:447` shown input → `rust/src/lib.rs:755` `for_approval` (enumerated list) → `rust/src/stream.rs:54` `safe` → `rust/src/agent.rs:903` prompt → `rust/src/session.rs:497` full input executed | How terminals render tag characters, variation selectors, U+00AD, Hangul fillers, U+2028/2029 and the other listed code points in the prompt line; whether the user's tool consumer interprets hidden code points | Scratch unit test: `safe(for_approval(...))` over a string holding U+E0049, U+FE0F, U+00AD, U+3164, U+206A, U+2028 shows each raw. Promote the prompt bytes and view them on terminals; replay with a dummy echo MCP server | List approved tools whose string arguments are stored or later read by an LLM; with a dummy note, check whether tag-encoded content is decoded by that consumer |
| 8 | State-directory gate checks only the leaf; later by-name opens follow a directory another account can swap through a writable ancestor | `rust/src/history.rs:15` `state_dir` → `rust/src/history.rs:58`/`:85` leaf-only `private_state` → `rust/src/lib.rs:456` reopen by name → `rust/src/request.rs:95` container id sent | Whether a realistic setup puts an ancestor under another account's write control; whether the fresh-process races are winnable; whether the API accepts a container or session id from another workspace under the user's key | Disposable no-network VM with two dummy accounts and a non-sticky 0777 shared `XDG_STATE_HOME`. (1) Swap the directory during an interactive `-s` chat and check for `cntr_B` in the request log and in a readable `events.jsonl`. (2) Run an inotify-timed swap against a bare `resume` up to 50 times | Confirm whether supported setups ever put `XDG_STATE_HOME` or `~/.local/state` under a directory another account can write, and whether the APIs accept a container or session id from another workspace |
| 9 | `--mcp-tool` entries separated by tab or newline pass `config::check` but leave an `--mcp` server with every tool enabled | `rust/src/cli.rs:153` → `rust/src/config.rs:86` checks with `words` (space, tab, newline) → `rust/src/lib.rs:184` `join(" ")` → `rust/src/request.rs:158`/`:162` `toolset` splits on space only | Sandbox only (the request body shape is established from source) | Scratch unit test: `config::check` returns Ok for `"a.example:search\nb.example:read"`; `body()` gives the `b-example` toolset no `default_config`; leading-tab variant; two-flag control keeps `default_config {enabled:false}` on both | Against an owner dummy MCP server with read and write tools, confirm in the logged request that its toolset has no `default_config`. Optionally ask the model to list its tools. Invoke no write tool |
| 10 | Untrusted retrieved content can drive every tool of a credentialed `--mcp` server with no deterministic gate | `rust/src/lib.rs:500`/`:519` pause_turn loop → `rust/src/request.rs:35` `body` → `rust/src/config.rs:53` no composition check → `rust/src/request.rs:59` token → `rust/src/request.rs:161` `toolset` without `default_config` | Whether the model, given attacker-written `web_fetch`/`web_search` content or another server's `mcp_tool_result`, issues an unrequested `mcp_tool_use`. Owner intent: `README.md:34-42` discloses this default, so is a bare `--mcp` meant to approve every tool? | Unit test: `Args{web: true, mcp: [one host], mcp_tool: ""}`. Check that `tools` holds `web_search`, `web_fetch` and the toolset with no `default_config`, that the dummy token is attached, and that `config::check` returns Ok. Repeat with two hosts | Owner logging-only MCP tool plus an owner page that instructs a `note_write` call. Run `--web --mcp` without `--mcp-tool` and record whether `note_write` is called. Record the owner's decision on whether a bare `--mcp` approves every tool |
| 11 | Rust `--memory` access suffix other than exact `:ro` silently attaches the memory store without `read_only` | `rust/src/cli.rs:227` → `rust/src/agent.rs:473` checks only the id prefix → `rust/src/session.rs:162` exact `"ro"` match → `rust/src/agent.rs:700` POST `/sessions` | The provider's default access mode for a `memory_store` resource sent without `access`. Precondition: the user types a non-`:ro` suffix and untrusted content then steers a memory write | Scratch unit test: `create_body` with seven suffixes (`:ro`, `:RO`, `:readonly`, `:read_only`, `:`, `:rw`, `:ro:x`). Expect `read_only` only on `:ro` and `:ro:x`. Then do a replay run with `--memory memstore_b:RO` and check the logged body | On a new dummy memory store, create a session with no `access` field and GET it to record the effective access mode. Archive the session and delete the store afterwards |
| 12 | Custom-tool round cap (ROUNDS=8) is skipped for every `requires_action` round that also carries an `agent.tool_use` ask | `rust/src/agent.rs:745` → `rust/src/session.rs:54` `fold` → `rust/src/agent.rs:806` counts only when there are no asks → `rust/src/session.rs:495` → `rust/src/local_mcp.rs:308` `tools/call` | Whether the Managed Agents API ever ends a stream with one `requires_action` idle naming both an `agent.tool_use` that needs confirmation and `agent.custom_tool_use` events, and whether a model can be steered to do so every round | Replay 12 mixed rounds (one ask and one custom call each) with `ASK_CLAUDE_CONFIRM=allow` and a logging fake MCP server. Vulnerable: 12 `tools/call` and exit 0. Control (no asks): 8 calls, interrupt, exit 1 | On a test agent with an `always_ask` toolset tool and a logging custom tool, see whether one `requires_action` idle names both ids. If it does, run more than 8 rounds and watch for calls beyond 8 |
| 13 | A session stream that ends or fails before the idle/terminated event is reported as a completed turn, and no `user.interrupt` is sent | `rust/src/agent.rs:745` → `rust/src/agent.rs:1017`/`:1023` End/Failed set `done` → `rust/src/session.rs:51` no idle → `rust/src/agent.rs:829` `"" => 0` → `rust/src/agent.rs:676` session remembered | The client-side defect is determined by source; the impact depends on Managed Agents behaviour: whether a session keeps running (and spending, up to its budget) after the stream client disconnects without `user.interrupt`, and whether the stream endpoint ever closes before idle | Replay a two-line stream with no idle: expect exit 0, `partial` on stdout, a `sesn_FIXTURE01` stub, an empty event-log error, and no `user.interrupt`; a unit-level `Agent::turn` assertion is the fix regression | On a dummy session with a small budget, record whether it keeps running with rising `list_cost` after the stream closes mid-turn without an interrupt, and whether the stream has a server-side lifetime |
| 14 | Raw-substring turn-end framing can be triggered by a nested object in model-controlled tool input, so the turn is reported as a clean exit 0 while the session is still running | `rust/src/agent.rs:1113` `ToTurnEnd::read` → `rust/src/agent.rs:1129` `ends_turn` substring → `rust/src/agent.rs:1039` → `rust/src/session.rs:51` top-level type only → `rust/src/agent.rs:829` | Whether the API emits model-supplied tool input containing a nested `{"type":"session.status_idle"}` verbatim and compactly in the event line, with no whitespace and no rewriting by schema validation | Replay `nested.sse` (custom call with the nested object, then `AFTER`, then the real idle). Expect exit 0, `AFTER` never printed, only the create body logged, and a stub. A string-valued control reaches the ROUNDS cap and exits 1 | On a disposable test agent, capture with curl the raw SSE line of a custom call whose free-form input holds the nested object. Record whether the compact substring is present, then archive the session |
| 15 | A Messages reply cut off mid-stream is saved to the chat and logged as a successful turn, and the process exits 0 | `rust/src/transport.rs:329` → `rust/src/lib.rs:328`/`:345` `exchange` treats Failed like End → `rust/src/lib.rs:592` `reply` → `rust/src/reassemble.rs:93` → `rust/src/lib.rs:500`/`:518`/`:540` commit, return 0 | Sandbox only. The ureq 3.5.2 / rustls 0.23.46 transport variant was established from pinned registry source; it decides which branch fires, not reachability | Replay a text stream without `message_delta`/`message_stop`; a `rust/tests/http.rs` Mock reply closed mid-body; a `-p openai` stream without `finish_reason`/`[DONE]`. Expect exit 0 and a saved partial assistant turn in each | None (the record has no deployment plan) |
| 16 | Model-sized `tools/call` write to a stalled `--local-mcp` server blocks the main thread with no deadline, and Ctrl-C/SIGTERM/SIGHUP cannot interrupt it | `rust/src/agent.rs:810` → `rust/src/session.rs:497` → `rust/src/local_mcp.rs:318`/`:307` `LocalMcp::call` → `rust/src/local_mcp.rs:181` blocking `write_all` | Whether real stdio MCP servers stop draining stdin during a `tools/call` past the timeout (and the trigger needs more than the free pipe capacity unread); whether an unanswered `requires_action` session keeps charging or holds a container | `rust/tests/agent.rs` harness: a fixture server stops reading stdin; two `slow` calls, the second about 128 KiB; `ASK_CLAUDE_LOCAL_MCP_TIMEOUT=1`; SIGINT at about 3 s. The failing result is no exit within 5 s; killing the fixture group is the recovery path | Check Managed Agents docs or dummy-session billing for whether an unanswered `requires_action` session accrues cost or holds a container, and whether users' stdio servers read stdin during `tools/call` |
| 17 | `agent tools` prints local MCP tool metadata to the terminal with C1 controls and bidi overrides left raw | `rust/src/local_mcp.rs:155` handshake → `rust/src/local_mcp.rs:238` `definitions` → `rust/src/agent.rs:139`/`:140` `to_string_pretty` with a DEL-only rewrite | Whether terminals act on UTF-8 C1 code points or apply bidi reordering; whether users run `--local-mcp` commands that relay third-party `tools/list` metadata | Fixture `tools/list` whose description holds U+009B and U+202E; pipe `agent tools` output through `od -An -tx1`; expect raw `c2 9b` and `e2 80 ae`, while BEL and DEL appear escaped | On the user's terminals, run the fixture with stdout on the terminal and record CSI and bidi behaviour; list `--local-mcp` commands that relay remote or third-party metadata |

## 6. Hardening notes and positive source patterns

### Hardening notes

These notes are not findings. Hunters and critics recorded 79 hunter notes plus 1 critic note, which are grouped and deduplicated here. Where a note sketches a fix for an open lead, the lead is named. That is a fix direction for an unconfirmed lead, not a confirmed fix.

**Approval prompt and gate (`rust/src/session.rs`, `rust/src/agent.rs`, `rust/src/local_mcp.rs`)**
- Show the tool name through `for_approval`-style escaping and key the in-memory "always" grant by the raw name. Today two names that differ only in zero-width, C0/C1 or bidi code points show the same and share one key. This is not reachable from model output, because `has()` matches the raw name against the user's own server list.
- Show the value actually sent. A null or false input is shown as `null`/`false` but sent as `{}` (`rust/src/local_mcp.rs:307`).
- Cap the length of the approval line, or show the input's size and require the user to expand it. A very large input pushes `allow <name>` off screen (`rust/src/session.rs:379`).
- Large integers in an ask's input are shown as rounded f64 because serde_json lacks `arbitrary_precision` (`rust/Cargo.toml:28`), while the server runs the exact value. Consider showing the raw number text.
- Have `ask_tty` open `/dev/tty` read-write and write the prompt to that handle. Flush pending input with `tcflush(TCIFLUSH)` before asking, and avoid Pipe mode when stdin is a terminal. These are fix directions for Leads 4, 5 and 6.
- State in the prompt text that an "always" grant lasts for the process and survives `/new`.
- Add a per-turn cap on calls, not only on rounds. One `requires_action` stop can carry any number of custom calls (see Lead 12).
- In a mixed round, answer every pending id the client does not pick (for example `agent.mcp_tool_use` with an ask permission) with an explicit deny. Or interrupt and name the unanswered ids on stderr. Also log asks or custom calls that are skipped for a null, false or empty id.
- Dispatch currently checks the live server's `tools/list` only, not the custom tools registered on the agent (`rust/src/session.rs:484`). Consider also requiring registration. The call stays gated either way.

**Multiagent and duplicate events**
- Keep only the first event per id in `session::fold` (and `lib/session-fold.jq`), and skip ids already answered in a round. Deny every copy of an id whose name or input differ between copies. This also removes the reuse of one JSON-RPC id on the local server after a late reply.
- Carry `session_thread_id` and the thread's agent name into the fold, the prompt and the "always" key. Prefix sub-thread output. Include `session_thread_id` in answers if the provider routes by it; an owner check against a recorded multiagent fixture would settle this.
- Show `mcp_server_name` in `(mcp <name>)` so that API-side MCP activity can be attributed (`rust/src/session.rs:18`).

**Session and Messages stream outcome**
- Session turns:
  - Add an `ended` flag set only when `ends_turn` matched. When it is missing, log the cut, record an `incomplete` error kind, post `user.interrupt` and return 1. Map a null `stop_reason` to 1.
  - Print `got.failure` even when data arrived.
  - Skip the pending POST, or interrupt, when the stream was already dead before the POST (`rust/src/agent.rs:754-774`).
  - These are fix directions for Lead 13.
- Messages turns:
  - Track `failed` and `stopped` (`message_stop` seen) in `Received`, and have `reply()` return a `truncated` failure so that `call()` rolls back.
  - Record `truncated` or `read_failed` in the event log.
  - Strip the internal `partial` key and drop incomplete tool or thinking blocks, so a cut round cannot leave a chat that the API then refuses.
  - These are fix directions for Lead 15.

**Local MCP process (`rust/src/local_mcp.rs`, `rust/src/main.rs`)**
- Write to the server through a writer thread, or poll a non-blocking fd with the timeout and `interrupted()` checks. The method-not-found reply to server requests takes the same blocking path. This is the fix direction for Lead 16.
- Send `notifications/cancelled` on a late reply or an interrupt, and bound a whole call by one deadline instead of resetting it per line.
- Cap the length of stdout lines from the server; `read_until` and an unbounded channel grow without limit.
- Reset SIGCHLD to `SIG_DFL` in `process_defaults`, or call `try_wait()` before signalling. An inherited `SIG_IGN` lets the kernel reap the server early, after which `stop()` could signal a reused same-user group.
- Remove ask-claude's own credentials (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `ASK_CLAUDE_MCP_TOKEN_*`) from the child's environment.
- Consider passing the server's inherited stderr through `safe` line by line.
- Consider requiring the `--local-mcp` flag explicitly instead of honouring `ASK_CLAUDE_LOCAL_MCP` alone (`rust/src/cli.rs:233-238`).

**State files and logs**
- Keep the verified state-directory descriptor and use `openat`/`mkdirat`/`renameat` relative to it, or check every ancestor. Replace the by-name `set_permissions` in the EACCES branch with `fchmod` on an `O_PATH|O_NOFOLLOW` descriptor. This is the fix direction for Lead 8.
- For `events.jsonl`, fstat the opened file and require a regular file owned by euid with `nlink` 1, opened `O_NONBLOCK`.
- Open the request and beta logs with `O_NOFOLLOW|O_NONBLOCK` and check `S_ISREG`. Have `private()` reject group or other write (`0o066`), not only read. Give the beta log the same owner check as the request log.
- Shape-check the container id read back from `<chat>.container`.
- Downloads (`rust/src/agent.rs#download`):
  - Cap the downloaded size, or check free space against `size_bytes`.
  - Remove the temp file on interrupt.
  - Use `renameat2(RENAME_NOREPLACE)` (or `link` then `unlink`) when `--force` is absent.
  - Give a clearer error for names near `NAME_MAX`.
- Escape a backslash in the multipart filename as `%5C` (`rust/src/transport/sessions.rs#form`).

**Transport, credentials and environment seams**
- Announce a non-default `OPENAI_BASE_URL` on stderr as `ANTHROPIC_BASE_URL` already is (`rust/src/config.rs:178-189`). This matters more because loopback roots are cleartext and could be bound by another local account if the port is free.
- Print one line when `ASK_CLAUDE_REPLAY` is active, or compile `ASK_CLAUDE_REPLAY`, `ASK_CLAUDE_REQUEST_LOG` and `ASK_CLAUDE_BETA_LOG` behind a test feature.
- Bound response sizes per line and per body, including after gzip expansion (`into_reader()` is unlimited).
- Set `hide_env_values` on env-backed clap arguments so that `--help` does not echo a base URL that carries a gateway token.
- For `--repo`, prefer a dedicated variable or document a fine-grained, single-repository, read-only token instead of the ambient `GITHUB_TOKEN`.

**CLI value validation**
- Refuse duplicate MCP server names that come from folded hosts.
- Compute the per-server token names in `config::check` with the same split that `request::token_variable` uses, so that its remediation message is right.
- Reject `--repo` owner or name values that are `.`, `..` or start with `.`.
- Refuse duplicate `--memory` ids that have different access.
- Require at least one id character after `memstore_` and `vlt_`.

**Display-only output**
- `safe` keeps U+00AD, U+2028/2029, U+200B-U+200F, U+2060-U+2064 and U+FEFF. Escape or drop them in listings whose rows users copy into arguments (`agent list`, `agent files`, `list`).
- Show a `web_fetch` host cut at `/?#@`, taking the part after the last `@` (`rust/src/session.rs:14`).
- Escape `agent tools` output as `for_approval` does, which keeps the JSON valid and makes it inert on a terminal (fix direction for Lead 17).
- `agent trace` prints `ASK_CLAUDE_WORKSPACE` without `safe` (`rust/src/agent.rs:241-247`).
- The advisor beta can be added by a substring match in history text, which costs only an extra header.

**OpenAI-compatible translator (`rust/src/openai.rs`)**
- Drop, or treat as a parse error, any `data:` line the translator cannot parse, instead of passing it through raw. Today a lone low surrogate lets a native Messages event bypass the text-only translation.
- Map unknown `finish_reason` values to a non-continuing value, so that `pause_turn` and `refusal` are not taken from the endpoint.
- Restrict a copied `role` to `user`/`assistant`.
- Fail the fold on a non-string text delta instead of storing `""` while printing the JSON.

**Signals and FFI**
- Check `SIG_ERR` when restoring SIGPIPE (`rust/src/local_mcp.rs:180-186`), as `rust/src/event.rs:409` does.
- Restore fd 1 with a Drop guard in `rust/src/chat.rs:140-147`.
- Ignore SIGPIPE while a `LocalMcp` is alive and map EPIPE on stdout to exit 141.
- Ignore SIGXFSZ consistently around log and chat writes.
- Let a second signal force an immediate exit during synchronous session calls.

**Supply chain**
- Add a `cargo-deny` configuration in `rust/` (crates.io-only sources, advisories).
- Add `--locked` to `cargo build`/`cargo test` in the `justfile`. The `justfile` is outside this run's scope, so this is noted only.

### Positive source patterns (controls that held)

These ledger checks closed `covered` on source alone.
- **Dependencies.** All 176 locked packages come from crates.io with checksums, and there are no git, path, patch or alternate-registry sources. Direct dependencies are pinned with `=`. The documented install is `cargo install --locked --path rust`. The package has no `build.rs`.
- **Ids and paths.** Session ids pass `id_ok` (`sesn_[A-Za-z0-9_-]{1,128}`) before they name a stub or a URL. Argv session and chat names select only listed entries. File ids are charset-checked before they reach `/v1/files/<id>/content`.
- **Downloads.** The destination is a user-typed single cwd name: no `/` and no leading `.` or `-`. It is written through a `create_new` 0600 temp file and a rename, never through an existing entry. The multipart filename escapes `"`, CR and LF, and the boundary is random and checked against the body.
- **Key routing.** `base_url_gate` refuses any `@` and allows only `https://` or exact-loopback http. Both transports set `max_redirects(0)`, and the key is never in a URL or the request log. `ANTHROPIC_BASE_URL` taken from the environment is never sent the OpenAI key. Every door that sends a key goes through `config::check`.
- **Delegated resources.** `--repo` always yields a `github.com` URL. `GITHUB_TOKEN` is read only with an explicit `--repo`. Vault ids and the upload `file_id` cannot be widened by API or model input.
- **History resend.** `resume -p openai` keeps only top-level text blocks. The stored role is fixed in code by every writer. No MCP token reaches the OpenAI path.
- **Logs.** Every API-originated field in `events.jsonl` is reduced to an identifier, number or host shape, and each line is one JSON object. The request log is refused unless it is owned by euid and not group- or other-readable, and that is re-checked by fstat before each write.
- **Paid loops.** `pause_turn` is capped at 9 requests per user turn. The advisor cap of 3 is recomputed every round and cannot be reset by reply content.
- **Local MCP.**
  - Tool arguments travel only as JSON values in one framed line, never as argv or shell text.
  - The approved object is the dispatched object.
  - `has()` is a raw-name allowlist.
  - MCP metadata never authorizes a call.
  - One request is outstanding at a time, so replies correlate by id.
  - Server-initiated requests are refused with -32601.
  - The command comes only from the user.
  - The child runs in its own process group, so it cannot read the approval answer.
  - `stop()` cannot hit a recycled group under the default SIGCHLD disposition, and every returning path stops the server.
- **Session events.**
  - `agent.mcp_tool_use` and unknown pending types never receive an allow.
  - A round with only unanswerable ids posts `user.interrupt` and exits 1.
  - Each duplicate copy of an event is gated separately.
  - Thread and workflow events cannot end the primary turn.
  - The client never redelivers an event.
- **Streams.** Model text cannot break SSE framing. The printer and the fold read the same decoded value. The decoder and fold have no reachable panic.
- **Terminal output.** `safe` removes C0 (except tab and LF), DEL, C1 and the nine bidi controls, and it is applied on every printer of API, model or session text except `agent tools` (Lead 17). The approval input is escaped through `for_approval` (Lead 7 covers its gaps).
- **Replay seam.** `ASK_CLAUDE_REPLAY` keeps `config::check` and `need_key` in front, sends no credential, and cannot escape its directory on download.
- **FFI.** Signal handlers only store an atomic flag. Signal and descriptor swaps happen on the main thread. The `kill(-group)` guard rules out group 0, negative or overflowed values.

## 7. Coverage summary

Counts are from `coverage-ledger.json` (63 units).

| Status | Units |
|---|---|
| covered | 26 |
| candidate (all ended as `needs_validation` records) | 17 |
| blocked | 1 |
| out_of_scope | 19 |
| deferred | 0 |

- **Blocked.** The surface is all files under `rust/`: the known-CVE status of the 176 crates.io packages in `rust/Cargo.lock`. No RustSec advisory database or `cargo-audit` was available offline, and no sandbox allowed fetching one. **Owner command:** run `cargo audit` (or `cargo deny check advisories`) against `rust/Cargo.lock` with a current advisory database.
- **Merged fingerprint.** `rust/src/agent.rs:ask_tty:buffered-typeahead-answers-confirmation` was merged into `rust/src/agent.rs:ask_tty:confirmation-consumes-type-ahead` (Lead 5). They share a root cause: `ask_tty` reads `/dev/tty` without discarding input typed before the prompt. The kept candidate had the stronger evidence.
- **Important exclusions.**
  - Everything outside `rust/` (part 1), including CI, the `justfile` and the bash oracle.
  - Companion blocks that were not selected: client-side (no browser or webview), cloud and deployment (no IaC, IAM or container in `rust/`), data isolation (single-user local store), and the supply-chain CI/release blocks (`.github/` and `justfile` are outside `rust/`).
  - API-side behaviour of the Managed Agents and Messages services, which appears only as owner-observed checks.
- **Hunting waves and critics.** Five waves were needed. The post-wave critics reported missing units as follows: wave 1, 4; wave 2, 2; wave 3, 1; wave 4, 4; wave 5, 0. None reported reassignments. **Final critic result:** the post-wave critic after wave 5 and the separate final-clean critic both returned no missing units and no reassignments, and both called for the run to stop.
- **Validation state.** Every candidate unit has a final record, independently verified in Phase 3 and Phase 5. All 17 records are `needs_validation`. Within the `rust/` scope, coverage is limited by two things: no target code was executed, and the one RustSec check is blocked.

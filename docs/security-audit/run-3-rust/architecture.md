# Architecture: ask-claude Rust port (run 3, scoped to `rust/`)

## 1. Product, principals, authority, protected resources

`rust/` is the Rust port of the bash `ask-claude`: a single-user CLI with no listener. It does Messages one-shot/chat (SSE), `list`/`resume` of stored chats, Managed Agents sessions (`agent`: create/stream/turn, `files`, `budget`, `archive`, `trace`, `tools`), a Rust-only `-p openai` translator, and `--local-mcp`, which spawns `sh -c <command>` as a stdio JSON-RPC MCP server for the agent's custom tools.

Principals:
- Local user (argv, stdin, `/dev/tty`): full trust.
- Process environment: the user's, but often set for other tools.
- API endpoint at the configured root: remote.
- Model/agent events: untrusted.
- Third parties: remote `--mcp` servers, web content, GitHub.
- The local MCP child: the user's command, but its output is data.
- Other local accounts.

Protected resources:
- Credentials: `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`, `ASK_CLAUDE_MCP_TOKEN[_<HOST>]`, `GITHUB_TOKEN`.
- Local actions: MCP tool calls, files written into the cwd.
- State: chats, the container id, session stubs, `events.jsonl`, the request log.
- Terminal integrity and approval intent.

## 2. Comparable baseline

The bash script plus `lib/*.jq` is the declared oracle (out of scope). Goldens hold the two to the same bytes, so a shared defect pattern strengthens a Rust finding rather than excusing it. curl/jq semantics are mimicked deliberately: no redirects, timeouts, multipart escaping, `tojson`.

## 3. Stack, deployment, offline limits

- Rust 2024, a single binary from `rust/src/main.rs`. Direct dependencies are `=`-pinned and `rust/Cargo.lock` is committed.
- ureq 3.5.2 with rustls and bundled `webpki-roots`, gzip on, and proxy taken from the environment by default.
- Proc macros and the `ring` build script run at build time.
- All `unsafe` is libc FFI: umask, signal/sigaction, dup/dup2, kill of the process group, geteuid, localtime_r.
- Installed with `cargo install --path rust`. No server or container.
- No OS sandbox and no built binary: nothing is built or run, so every check is source-only, and a decisive runtime fact becomes `needs_validation`.
- `rust/tests/` and `rust/benches/` are test harness, not shipped.

## 4. Entry surfaces and key paths

- **API responses**: SSE lines go through `transport::lines`, `sse::decode`, `reassemble::message`, then `stream::line`/`safe`, the chat file and `<chat>.container`. Session JSON goes to `id_of`/`dollars`/`file_rows`.
- **Session event stream**: `ToTurnEnd`/`ends_turn` (raw substring framing), then `session::fold`, then `session::answer`. Answers go through `Gate::confirm` and then `user.tool_confirmation` or `LocalMcp::call`.
- **Downloads**: `agent files` takes the API file list and bytes through `agent::download` into a cwd temp file, then a rename.
- **Local MCP stdout**: the `LocalMcp::read` id-text match, the `tools/list` `has()` allowlist, and `result`, whose output goes to the session.
- **CLI/env**:
  - `--base-url`/`ANTHROPIC_BASE_URL`/`OPENAI_BASE_URL` go through `base_url_gate`.
  - `--mcp`/`--mcp-tool` go through `config::check`, then `request::host`/`token_variable`/`toolset`.
  - `--memory`/`--repo`/`--vault` go into `session::create_body`.
  - `ASK_CLAUDE_CONFIRM`, `ASK_CLAUDE_REPLAY` (live in release builds), `ASK_CLAUDE_REQUEST_LOG`, and the implicit `*_PROXY`.
- **Files**: `-f` (inlined or uploaded), the state dir from `XDG_STATE_HOME`/`HOME`, and `/proc/*/stat` (`group_alive`).
- **Resume**: `resume -p openai` resends stored history through `openai::body`.

## 5. Trust boundaries and strongest control

| Boundary | Strongest source control |
|---|---|
| Model to local action | `Gate::confirm` (tty, in-memory per-name "always", `for_approval` on input); `ROUNDS` cap (skipped when asks are present) |
| API text to terminal | `stream::safe` (keeps zero-width characters; `agent tools` bypasses it) |
| Key to network | `base_url_gate` (https, or loopback http; refuses `@`); `max_redirects(0)`; no proxy override |
| MCP tokens to server | `token_variable` fold, `url_host` cut at `/` or `:`; `--mcp` is only prefix-checked |
| API names/ids to filesystem/URL | `id_ok`, the file-id charset, the `download` name filter, `create_new` 0600 plus rename |
| Other accounts to state | `private_state` (leaf `O_NOFOLLOW` fchmod/fstat; ancestors unchecked, later opens go by name) |
| Local MCP child | own process group and stop; full environment inherited; ids matched as text |
| Request log | owner/mode fstat; no `O_NOFOLLOW`; holds tokens |

## 6. Starting paths

- `rust/src/session.rs`, `rust/src/agent.rs`
- `rust/src/local_mcp.rs`, `rust/src/request.rs`, `rust/src/config.rs`
- `rust/src/lib.rs`, `rust/src/stream.rs`, `rust/src/sse.rs`, `rust/src/reassemble.rs`
- `rust/src/transport.rs`, `rust/src/transport/sessions.rs`
- `rust/src/history.rs`, `rust/src/event.rs`, `rust/src/openai.rs`
- `rust/src/chat.rs`, `rust/src/main.rs`, `rust/Cargo.toml`, `rust/Cargo.lock`

## 7. Prior-run consequences

Run 2 (bash-scoped, source `9c21790e`) left 8 open records, all against bash/jq. No prior ledger covered `rust/`, so every unit is `new`.

**Same root cause carried into Rust (high-priority units):**
- **state-dir owner/parent**: `history.rs#private_state` checks only the leaf. It is narrowed, and the `.always` sink is gone.
- **custom-round cap skipped when asks are present**: `agent.rs#Agent::turn`.
- **MCP token host-variable collision**: `request.rs#token_variable`.
- **MCP userinfo host mismatch**: `lib.rs#url_host` together with `config.rs#check`.
- **toolset unconfirmed with untrusted retrieval**: `request.rs#body`/`toolset`.
- **memory suffix fails open**: `session.rs#create_body`.

**Partly fixed:** confirm-display bidi. The input is escaped by `for_approval`. A residual remains: the tool name goes through `safe` only, and "always" is keyed by that shown name.

**Fixed:** dollars-tonumber. `agent.rs#dollars` returns `None`. This is a revalidation target only.

Bash-side records stay out of scope. No same-source confirmed exclusions apply.

## 8. Companion selection

**Selected:**
- **AI-AND-LLM**: model events drive local MCP calls and server-tool confirmations, MCP token/toolset binding, delegated session resources and history resend.
- **DESKTOP-MOBILE-AND-LOCAL-IPC**: local IPC client to a spawned stdio server; state-dir and cwd writes exposed to other accounts; the request log as a local secret store.
- **WEB-PROTOCOL-AND-AUTH** (API-key classes only): API key transport rules and the proxy environment.
- **PROTOCOLS-RPC-AND-MESSAGING**: SSE and session-stream framing; JSON-RPC reply correlation.
- **RESOURCE-EXHAUSTION-AND-AVAILABILITY**: uncapped `read_until`/`read_to_end`/`io::copy` with gzip; model-driven paid rounds.
- **MEMORY-SAFETY-AND-BINARY** (FFI/concurrency classes): `unsafe` libc calls changing process-wide signal and fd state while worker threads run; kill by process group from `/proc` data.
- **SUPPLY-CHAIN-AND-RELEASE** (dependency classes): `rust/Cargo.lock` resolution and build-time proc macros/build scripts.

**Not selected:**
- **CLIENT-SIDE**: no browser, webview or DOM.
- **CLOUD-AND-DEPLOYMENT**: no IaC, IAM, container or ingress in `rust/`.
- **DATA-ISOLATION-AND-LIFECYCLE**: single-user local store, with no tenants, export or retention promise.
- **SUPPLY-CHAIN CI/release blocks**: `.github/` and `justfile` are outside `rust/`, so they are recorded out of scope.

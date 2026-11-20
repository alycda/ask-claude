# Security audit: ask-claude (run 2, scoped)

**This is a scoped, partial pass.** It covers source changed since run 1 (88afb08) and the work run 1 left open. It does not re-review unchanged source, and it does not claim the rest of the repository is clean.

## Run

- **Profile:** `standard`, with Phase 3 candidate validation and Phase 5 record verification run as separate fresh agents.
- **Budget:** 32 agent invocations, 30 spent. The owner set 25 at the start and raised it to 32 after wave 1, when 8 candidates needed about 18 more agents with 13 left. The raise funded validation and the final-clean critic, not a second hunter wave.
- **Source ref:** `9c21790e527c3f8ca3b5f569d26f3a14e8f8c664` (clean). Outside `docs/plans/` and `tools/security-audit/`, it is identical to `8cbbcd5`, the commit the plan was written at.
- **Skill:** `cloudflare/security-audit-skill` at `c1c8a8c`, read from `~/security-audit-skill/src-c1c8a8c`, outside the target.
- **Execution:** sandboxed source-and-local-only. Target code ran only in `sandbox-run.sh` (Docker on this Mac, with a local image that was never pulled):
  - `--network none`
  - read-only rootfs, `/tmp`, `/dev/shm` and target; only the agent's scratch directory writable
  - an allowlisted environment with dummy secrets
  - 256 MiB, 0.5 CPU, 64 pids, 20 MiB per file, 60 s wall clock
  - uid 1000, all capabilities dropped

  The branch's `tools/security-audit/sandbox` is Linux-only (`unshare`) and was not used on darwin. No deployed endpoint, MCP server, or paid API was contacted. One verifier (`v02`) ran its own one-line jq `tonumber` probe on the host's jq 1.7.1 and 1.8.2 with a dummy string to measure version dependence. That probe ran no target code.
- **Harness limits:** the sandbox has jq 1.7 (the target pins 1.8.2) and no `usage` runtime. The script ran as `bash ask-claude` with `usage_*` preset. Anything `usage` itself would parse or refuse is not established. Hunters marked version-dependent results where they apply.
- **Prior run:** run 1's ledger and findings were read, and every record pointed at changed source, so nothing was carried or excluded from hunting. There were 10 changed-source revalidation units. The post-wave critic marked these run-1 records as fixed in current source:
  - `lib/request.jq:mcp_servers:shared-authorization-token`
  - `ask-claude.sh:base-url-gate:loopback-userinfo-bypass`
  - `ask-claude.sh:jq-arg-private-text-on-argv`
  - `ask-claude.sh:managed-agents-status-sinks-bypass-safe`
  - `ask-claude.sh:session_answer:tsv-double-escape-confirm-display`
  - `ask-claude.sh:session_confirm:always-grant-file-pid-reuse`
  - `ask-claude.sh:session_files/part-file-symlink-follow`
  - `ask-claude.sh:session_remember/unvalidated-api-session-id-path`
  - `tools/effver/check-effver:bump:parent-VERSION-arithmetic-injection`

  The other run-1 lead, `ask-claude.sh:state-dir/owner-and-parent-unchecked`, persists in part and is reported below. Run 1's 4 deferred units were all reviewed in this run.
- **Out of scope** (6 units, unchanged source or audit tooling):
  - justfile#recipes .env and recipe arguments / justfile#quote() and .env sourcing: scoped run: the run-1 rejected claim justfile:ask:unquoted-recipe-interpolation targets the unchanged ask recipe; the new recipes (unit-tests filter, pty-test, actionlint) are reviewed by the .github/workflows unit via starting path justfile
  - lib/reassemble.jq#assistant content blocks / ask-claude#call history append and resend: scoped run: run-1 covered this unit and lib/reassemble.jq is unchanged since 88afb08; the call-history path in ask-claude is left to the coverage critic
  - repository#all in-scope files / repository#basic exposure checklist: scoped run: repository-wide passes are not re-run; changed surfaces have their own units
  - repository#all in-scope files / repository#unassigned behaviour: scoped run: repository-wide passes are not re-run; changed surfaces have their own units
  - tools/effver/bump-effver#VERSION and date argv / tools/effver/bump-effver#version parse: scoped run: run-1 covered this unit and tools/effver/bump-effver is unchanged since 88afb08
  - tools/security-audit#audit tooling / run-metadata.json#out_of_scope_paths: audit tooling added in 9c21790e for this run; not part of the reviewed product
- **Deferred** (1):
  - tools/effver/effver.just#bump recipe: VERSION from bump-effver spliced into a sed script: budget_cannot_reserve_critics_and_validation: accepted by post-wave critic critic-w1; the owner-raised budget (32) funds validation of wave-1 candidates and the final-clean critic, not a second hunter wave. Hunter h6 observed GNU sed `e` executing from effver.just:7 under a crafted VERSION (agents/h6-ci-effver/artifacts/t3.out) but that is unvalidated.

## Security posture

Run 1's eight confirmed issues and its shared-MCP-token lead are fixed in current source, and the fixes held under hostile local inputs. The new `--local-mcp` path keeps the approved tool call and the executed call bound to the same JSON value. Its FIFO transport is private, and it reaps an uncooperative server's whole process group. The remaining problems are at the edges:
- loop and credential binding: 3 low-severity confirmed findings
- the confirmation display, state-directory ancestry and provider-default behavior: 5 needs-validation leads, each with an exact blocker

No finding gives a remote party code execution without a user approval.

## Confirmed findings

| Severity | Title | Boundary | Observed result |
|---|---|---|---|
| low | Endpoint-supplied cost amount reaches the terminal raw through jq 1.7's tonumber error message, bypassing safe | `ask-claude:837` | jq 1.7 sandbox: the agent turn printed `hello` on stdout and on stderr `jq: error (at <stdin>:1): Invalid numeric literal at line 1, column 2 (while parsing '<ESC>]0;V02-TITLE<BEL><ESC>[31mV02FORGED')` with 2 raw 0x1b bytes; the usage line was not printed (rc 0). |
| low | Custom-tool round cap is skipped for any round that also carries an agent.tool_use confirmation | `ask-claude:677` | Control: 8 tools/call reached the fake MCP server, 8 custom_tool_result bodies were posted, then '[stopped: requires_action after 9 custom tool rounds; interrupting]', exit 1. |
| low | Per-host MCP token is attached to any --mcp host whose name folds to the same variable (mcp-example.com receives the mcp.example.com token) | `ask-claude:316` | With only ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM set, --mcp https://mcp-example.com/sse produced mcp_servers=[{url:'https://mcp-example.com/sse', name:'mcp-example-com', authorization_token:'tok_FOR_mcp.example.com_ONLY'}] with exit 0. |

### Endpoint-supplied cost amount reaches the terminal raw through jq 1.7's tonumber error message, bypassing safe

`ask-claude:dollars-tonumber-error-bypasses-safe`: low (likelihood low, impact low)

The DOLLARS helper (ask-claude:161) converts the API's cost string with a bare tonumber. When the Managed Agents endpoint returns a non-numeric list_cost/max_list_cost amount, jq aborts the program before the trailing `| safe` runs. Under jq 1.7.x the error text written to stderr is `Invalid numeric literal ... (while parsing '<raw string>')` with the decoded string verbatim, so ESC/BEL/CSI/OSC bytes reach the user's terminal (title rewrite via OSC 0, colour/clear/forged text via CSI, OSC 52 clipboard writes where the terminal honours them). The expected status line is not printed. Three sinks: the session usage line after every `agent` turn (ask-claude:837-838), `agent list` (:940) and `agent budget` (:1058); none redirects jq's stderr. Under the pinned jq 1.8.2 the same error is printed JSON-escaped and truncated, so the leak depends on an older jq being first on PATH, which the script does not prevent (it checks only `command -v jq`). macOS 15 ships /usr/bin/jq 1.7.1-apple and Ubuntu 24.04 ships jq 1.7.1, and mise's pin applies only where mise is activated.

**Root cause.** safe is applied only to the success value of each status-line jq program. DOLLARS calls tonumber on an unvalidated endpoint string; jq 1.7 embeds the unparsed string raw in its error message, which jq prints on its own stderr, inherited by the terminal. lib/session-fold.jq:31 copies usage.list_cost.amount from the stream unchanged, and session_list/session_budget pass the GET /sessions and POST /sessions/<id> amounts straight to dollars. ask-claude:132 accepts any jq version.

**Lower-trust principal.** A malicious or compromised Managed Agents endpoint (lower-trust principal) answering a normal `ask-claude agent "..."`, `ask-claude agent list` or `ask-claude agent budget <sesn> <cents>` invocation by the local user

**Trace.**

1. **entrypoint** `ask-claude:488` (session_stream_open): curl reads the session event stream from BASE_URL (lower-trust endpoint) into the log; the session.usage event is endpoint-controlled
2. **propagation** `lib/session-fold.jq:31` (session usage fold): usage.list_cost.amount from the last session.usage event is copied into .usage.list_cost without type or shape validation
3. **propagation** `ask-claude:161` (DOLLARS): def dollars: (tonumber / 100) ... bare tonumber on the raw string, no try/? guard
4. **sink** `ask-claude:837` (session_turn usage line): jq -r "$SAFE$DOLLARS" '.usage | ... (.list_cost | dollars) ... | safe' >&2; tonumber errors before safe and jq 1.7 writes the raw string to stderr

**Conditions.**

- *third_party_dependency*: The Managed Agents endpoint at BASE_URL (or a proxy/party that controls its responses or session stream) returns a non-numeric list_cost/max_list_cost amount containing control characters
- *environmental_dependency*: A jq older than 1.8 is first on PATH when ask-claude runs (observed: sandbox jq 1.7 and macOS 15 /usr/bin/jq 1.7.1-apple both leak; the pinned jq 1.8.2 prints the value escaped as "\u001b..." and does not leak)
- *environmental_dependency*: Effect depends on the terminal: OSC 0 titles and CSI colour/clear work in most terminals; OSC 52 clipboard writes only where enabled

**Bounded local reproduction.**

1. Create a replay directory with 1.sse (agent.message, the session.usage event above, session.status_idle end_turn), create.json {"id":"sesn_FIXTURE01","status":"running"}, sessions.json and update.json holding the payloads as JSON text with \u escapes (not raw bytes).
2. In the no-network sandbox (bash 5.2, jq 1.7) run `bash ask-claude` with ANTHROPIC_API_KEY=sk-ant-test-not-a-key, ASK_CLAUDE_REPLAY=<dir>, ASK_CLAUDE_EVENTS=off, usage_model=m usage_max_tokens=1000 usage_prompt=hi and usage_cmd=agent (plus usage_agent, usage_environment, usage_budget=100); repeat with usage_cmd='agent list', and with usage_cmd='agent budget' usage_session=sesn_A usage_cents=5. Capture stdout and stderr separately.
3. Inspect both streams with od -c and count 0x1b bytes; separately run `tonumber` on the same string with jq 1.8.2 to compare.

**Observed.** jq 1.7 sandbox: the agent turn printed `hello` on stdout and on stderr `jq: error (at <stdin>:1): Invalid numeric literal at line 1, column 2 (while parsing '<ESC>]0;V02-TITLE<BEL><ESC>[31mV02FORGED')` with 2 raw 0x1b bytes; the usage line was not printed (rc 0). `agent list` stderr carried <ESC>]0;V02-LIST<BEL><ESC>[31mX (2 ESC, rc 0); `agent budget` stderr carried <ESC>]52;c;VjAy<BEL> (1 ESC, rc 1). Stdout had 0 ESC in all runs. Host macOS /usr/bin/jq 1.7.1-apple shows the same raw echo for tonumber; pinned jq 1.8.2 prints `string ("\u001b]0;V02\u0007\u001b...") cannot be parsed as a number` with no raw control byte.

**Priority.** Likelihood: Requires a malicious or compromised Managed Agents endpoint/proxy (a principal that already controls reply text) and a pre-1.8 jq first on PATH; the pinned 1.8.2 does not leak, though common system jqs (macOS 15, Ubuntu 24.04) are 1.7.1 and the pin is not enforced outside a mise-activated shell Impact: Terminal integrity only: title/colour/screen manipulation, forged text, OSC 52 clipboard writes where allowed; the real cost line is suppressed. No code execution or secret disclosure on this path alone

**Smallest fix.** Make dollars total on any input so safe always runs: parse with `tonumber?` (or a numeric regex) and fall back to '?'. Optionally also send these status-line jq programs' stderr to /dev/null, and/or require jq >= 1.8 at startup. Regression test: a fold, a sessions page and a budget response whose amount is "\u001b[31mX" each print a line containing '?' and no 0x1b byte on either stream, under both jq 1.7 and 1.8.

`ask-claude`:

```
DOLLARS='def dollars: (tonumber? // null) as $n | if $n == null then "?" else ($n / 100) as $d | "$" + ($d | floor | tostring) + "." + (($d * 100 | round) % 100 | tostring | if length < 2 then "0" + . else . end) end; '
```

### Custom-tool round cap is skipped for any round that also carries an agent.tool_use confirmation

`ask-claude:session_turn:custom-round-cap-skipped-when-asks-present`: low (likelihood low, impact low)

session_turn caps at 8 the custom-tool rounds the script answers by itself. It counts a requires_action round only when the fold has no agent.tool_use asks (ask-claude:828), but session_answer then runs every custom call in that round anyway (ask-claude:741-760). A session stream (the model, or the endpoint at BASE_URL, which can fabricate any event) that puts one agent.tool_use next to its agent.custom_tool_use calls in each requires_action keeps the counter at 0. With confirmations auto-answered (ASK_CLAUDE_CONFIRM=allow, or after an 'a(lways)' answer), the script sends tools/call to the --local-mcp server round after round until the stream stops by itself. The cap exists to stop that loop. Separately, no limit applies to the number of custom calls in one round, so even the intact cap bounds rounds, not executions.

**Root cause.** ask-claude:828 `[ "$(jq '.asks | length' <<<"$fold")" -gt 0 ] || rounds=$((rounds + 1))` exempts the whole round from the count when any ask is present. The rationale in the comment at :826-827 (confirmations are the user's) applies to the asks only, not to the custom calls in the same round, which session_answer still executes through session_confirm (:752) and local_mcp_call (:677).

**Lower-trust principal.** A session stream (model output steered by injected content, or the endpoint at BASE_URL) that wants to keep invoking a local MCP tool after the script's 8-round limit.

**Trace.**

1. **entrypoint** `ask-claude:488` (session_stream_open): The session event stream from BASE_URL (lower-trust: the model's output or the endpoint itself) is written to the log that the fold reads.
2. **propagation** `lib/session-fold.jq:24` (session-fold.jq asks): An agent.tool_use whose id is in the requires_action idle's event_ids goes into .asks.
3. **propagation** `lib/session-fold.jq:25` (session-fold.jq custom): An agent.custom_tool_use in the same event_ids goes into .custom in the same fold.
4. **propagation** `ask-claude:828` (session_turn): Because .asks is non-empty, rounds is not incremented, although .custom is also non-empty.
5. **propagation** `ask-claude:829` (session_turn): rounds stays 0, so the -le 8 check passes and session_answer is called again on every such round, with no other bound in the loop.
6. **propagation** `ask-claude:752` (session_answer): Each custom call goes through session_confirm, which returns allow without a prompt under ASK_CLAUDE_CONFIRM=allow or when the always-grant file exists.
7. **sink** `ask-claude:677` (local_mcp_call): tools/call with stream-chosen name (from the server's listed tools) and arguments is written to the local MCP server process, which runs with the user's privileges and environment.

**Conditions.**

- *system_configuration*: Run with --local-mcp (agent subtree), and confirmations auto-answered: ASK_CLAUDE_CONFIRM=allow (the documented unattended mode) or an earlier 'a(lways)' answer in the same process. In ask mode with a tty every call is prompted; without a tty every call is denied.
- *third_party_dependency*: The stream must list an agent.tool_use and an agent.custom_tool_use in the same requires_action idle. The endpoint at BASE_URL can fabricate this; whether the real Managed Agents API batches both kinds in one pause under a steered model was not observed. The paired agent.tool_use is itself auto-allowed, so it also runs in the session's container.

**Bounded local reproduction.**

1. Build two ASK_CLAUDE_REPLAY directories with 20 requires_action turn files and a 21st end_turn file: control has only the agent.custom_tool_use per turn; mixed has the custom call plus one agent.tool_use, both in event_ids.
2. Run bash ask-claude in the offline sandbox with usage_cmd=agent, usage_local_mcp=tests/fixtures/mcp/fake-server, ASK_CLAUDE_CONFIRM=allow, FAKE_MCP_LOG and ASK_CLAUDE_REQUEST_LOG set, once per directory.
3. Count tools/call lines in the fake server log and user.custom_tool_result entries in the request log.

**Observed.** Control: 8 tools/call reached the fake MCP server, 8 custom_tool_result bodies were posted, then '[stopped: requires_action after 9 custom tool rounds; interrupting]', exit 1. Mixed: 20 tools/call, 20 custom_tool_result and 20 tool_confirmation bodies; the cap never fired and the run ended normally (exit 0) at the end_turn idle the stream chose to send.

**Priority.** Likelihood: Needs --local-mcp, unattended confirmation (allow or always), and a stream that pairs both call kinds in every pause; the endpoint can do this, a normal model run is not shown to. Impact: Same-user scope: more executions of local tools the user already authorized as a class under auto-allow, plus more session spend. With the real API the session budget bounds spend; a malicious endpoint is not bound by it but already gets unbounded calls per round under the intact cap. The cap is a loop limit, not an authorization.

**Smallest fix.** Count every requires_action round that has custom calls, whether or not asks are present; rounds with asks only can stay uncounted. Optionally also cap the number of custom calls answered per round.

`ask-claude`:

```
      # custom tool calls are answered by this script, so every round that has
      # any is capped; rounds of confirmations only are the user's
      [ "$(jq '.custom | length' <<<"$fold")" -eq 0 ] || rounds=$((rounds + 1))
```

### Per-host MCP token is attached to any --mcp host whose name folds to the same variable (mcp-example.com receives the mcp.example.com token)

`lib/request.jq:mcp_token:host-variable-name-collision`: low (likelihood low, impact medium)

Each --mcp server's bearer token comes from ASK_CLAUDE_MCP_TOKEN_<HOST>. The variable name is built by changing every character outside [A-Za-z0-9_] to '_' and upper-casing the result. That mapping is not injective, so different DNS hosts share one variable: mcp.example.com and mcp-example.com both read ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM, and api.mcp.example.com, api-mcp.example.com, api.mcp-example.com and api-mcp-example.com all read ASK_CLAUDE_MCP_TOKEN_API_MCP_EXAMPLE_COM. A user who keeps ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM exported for mcp.example.com (the setup README.md:18-21 documents) and runs --mcp https://mcp-example.com/... gets that token placed in the lookalike server's authorization_token in the request body sent to the Messages API, whose MCP connector presents it to the server at that url. The shared-token refusal at ask-claude:125 does not fire, because it checks only the shared ASK_CLAUDE_MCP_TOKEN. When both hosts are given together, both entries carry the same token and the same server name.

**Root cause.** lib/request.jq:53 looks up env["ASK_CLAUDE_MCP_TOKEN_" + (host | ascii_upcase | gsub("-"; "_"))], where host (lib/request.jq:40) has already changed every character outside [A-Za-z0-9_-] to '-'. ask-claude:118-121 derives the same lossy name. '.' and '-' therefore map to the same character, distinct hostnames resolve to one environment variable, and nothing records which exact host a token belongs to.

**Lower-trust principal.** An attacker who owns mcp-example.com publishes instructions to use --mcp https://mcp-example.com/sse. The victim already has ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM exported for the real mcp.example.com server.

**Trace.**

1. **entrypoint** `ask-claude:113` (--mcp loop): Each user-supplied --mcp URL (usage_mcp) is iterated; line 114 checks only for an https:// prefix.
2. **propagation** `ask-claude:120` (--mcp loop): The bash side changes every character outside [A-Za-z0-9_] in the host to '_', so '.', '-' and '_' become indistinguishable in the variable name it reports.
3. **propagation** `lib/request.jq:40` (def host): jq folds the host with gsub("[^A-Za-z0-9_-]"; "-"), merging '.' and '-'.
4. **propagation** `lib/request.jq:53` (def mcp_token): The token is read from ASK_CLAUDE_MCP_TOKEN_ plus the upper-cased folded host with '-' turned into '_'; mcp.example.com and mcp-example.com both read ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM.
5. **propagation** `lib/request.jq:80` (mcp_servers): The entry for the lookalike URL gets authorization_token set to the other host's token, and the name (host) mcp-example-com, which duplicates the legitimate server's name.
6. **sink** `ask-claude:316` (transport): The body is sent with curl to the Messages API, whose MCP connector presents authorization_token to the server at the entry's url.

**Conditions.**

- *system_configuration*: The user keeps ASK_CLAUDE_MCP_TOKEN_<HOST> exported (for example in a shell rc), the documented setup for a server they use.
- *user_interaction*: The user passes --mcp with a URL whose host differs from the trusted host only where the trusted host has '.' and the lookalike has '-' (or vice versa), for example copied from attacker-written docs. Underscore variants fold too but are not valid DNS hostnames, so they are not a practical attack.
- *third_party_dependency*: The attacker controls such a lookalike host (a separately registrable name such as mcp-example.com, or a dash-variant subdomain). The Anthropic MCP connector sends authorization_token as the bearer credential to the url; this is documented connector behavior, not exercised here.

**Bounded local reproduction.**

1. In the sandbox, run bash /target/ask-claude with ANTHROPIC_API_KEY=sk-ant-test-not-a-key, usage_model=m, usage_max_tokens=1000, usage_prompt=ping, ASK_CLAUDE_REPLAY=/target/tests/fixtures/sse/mcp.sse, ASK_CLAUDE_REQUEST_LOG=/scratch/req.jsonl, the token variable from the payload and no other token variable.
2. Set usage_mcp to each payload URL in turn, then read .mcp_servers from the request log with jq.

**Observed.** With only ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM set, --mcp https://mcp-example.com/sse produced mcp_servers=[{url:'https://mcp-example.com/sse', name:'mcp-example-com', authorization_token:'tok_FOR_mcp.example.com_ONLY'}] with exit 0. Listing mcp.example.com and mcp-example.com together gave both entries that token and the name mcp-example-com, exit 0. With only ASK_CLAUDE_MCP_TOKEN_API_MCP_EXAMPLE_COM set, api-mcp.example.com, api.mcp-example.com and api-mcp-example.com each received tok_ONLY_api.mcp.example.com. Not observed: whether the API accepts two servers with the same name, and the connector actually sending the token to the url.

**Priority.** Likelihood: The attacker must control a dot/dash lookalike of a host the victim holds a token for, and the victim must pass that URL to --mcp while the per-host token is exported. Impact: The victim's bearer token for one MCP server is presented to the attacker's server, whose operator can then act with that credential within its scope.

**Smallest fix.** Bind each token to the exact parsed host. Use an injective variable encoding (for example, '.' to '__' and '-' to '_D_', or hex of the lower-cased hostname), or keep an explicit host record (e.g. ASK_CLAUDE_MCP_HOST_<VAR>) and refuse a URL whose exact host does not match it. Refuse two --mcp URLs that produce the same server name or token variable. Regression test: with only ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM set, --mcp https://mcp-example.com/x carries no authorization_token, and mcp.example.com together with mcp-example.com exits 2.

`ask-claude`:

```
# in the --mcp loop, after deriving h: refuse folded collisions
case " $mcp_vars " in *" ASK_CLAUDE_MCP_TOKEN_${h^^} "*) die "--mcp servers $v and an earlier one share the token variable ASK_CLAUDE_MCP_TOKEN_${h^^}" ;; esac
# and in lib/request.jq/ask-claude derive the variable injectively from the exact host, e.g. dots to '__' so mcp.example.com -> MCP__EXAMPLE__COM and mcp-example.com -> MCP_EXAMPLE__COM
```

## NEEDS VALIDATION

These are leads, not confirmed vulnerabilities, and carry no severity. Full traces are in `NEEDS-VALIDATION.md`.

| Lead | Trace | Blocker | Local next step | Owner-observed check |
|---|---|---|---|---|
| State-dir gate checks only the leaf $STATE, once, so an account that controls a non-sticky ancestor of XDG_STATE_HOME can swap the vetted directory after the gate and plant .always.<pid> | `ask-claude:523` | The sandbox runs one uid (1000) with all capabilities dropped. The decisive cross-account step was simulated with the same uid and not observed: a second account renames a uid-1000-owned 0700 directory inside a non-sticky parent that the second account owns or can write, then creates a replacement. Linux rename(2) normally allows a same-parent directory rename with write+search on the parent alone, and the sticky bit blocks it, but this is unverified here. | On a disposable, offline, multi-user Linux VM with dummy accounts A (victim) and B (attacker): as B, mkdir /tmp/ac-state && chmod 0777 /tmp/ac-state (no sticky bit). | The owner checks every machine or shared host where ask-claude runs for an XDG_STATE_HOME (or HOME) whose path has an ancestor that another local account owns or can write without the sticky bit, or that sits at a predictable path another account could pre-create, such as /tmp/<name>. |
| Tool confirmation prompt prints bidi overrides and zero-width characters raw and drops C1, so the rendered prompt can differ from the arguments that run | `ask-claude:677` | Whether the raw format characters change what the user sees depends on the user's terminal bidi implementation (for example VTE/GNOME Terminal with bidi on, Konsole, mlterm, iTerm2, Terminal.app) and font; the sandbox has no terminal renderer, so the visual reordering, and therefore an approval of a rendering that reads differently from the executed arguments, is not locally observable. Locally established only at byte level: on the real interactive prompt path U+202E/U+202C/U+200B are printed raw and U+0085 is dropped from the prompt while the local MCP server receives it. | Replay a one-turn Managed Agents stream (ASK_CLAUDE_REPLAY dir with 1.sse) containing agent.custom_tool_use {"id":"sevt_C1","name":"get_weather","input":{"path":"/home/u/\u202etxt.hs\u202c","note":"a\u200bb\u0085c"}} followed by session.status_idle requires_action with event_ids ["sevt_C1"]; run `bash ask-claude` with usage_cmd=agent, usage_local_mcp=tests/fixtures/mcp/fake-server, FAKE_MCP_LOG set, ASK_CLAUDE_CONFIRM=ask, under script -qec with 'y' fed after the prompt. | Owner-observed, non-destructive, on the terminals the user actually uses (at minimum the default terminal on the user's OS plus any bidi-capable one in use, e.g. |
| --mcp token variable comes from the text before the first ':' or '/', so https://mcp.example.com:443@evil.example/ carries the mcp.example.com token | `lib/request.jq:81` | Whether the Anthropic MCP connector accepts an https URL containing user info, and if so whether it connects to the RFC 3986 host (evil.example) and sends authorization_token as a bearer header, is provider behavior that cannot be observed offline. If the API rejects such URLs, the token never leaves Anthropic and the impact is nil. | Done (v06, independent): with only ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM set, replay request logs show authorization_token attached for https://mcp.example.com:x@evil.example/sse and https://mcp.example.com:443@evil.example/mcp, and not for https://mcp.example.com@evil.example/sse, https://mcp.example.com.evil.example/sse or https://mcp.example.com#@evil.example/sse (agents/v06/artifacts/userinfo-binding.out). | Owner-observed, non-destructive: the owner runs a test MCP endpoint they control at host B (logging request headers) and sends one Messages request with mcp_servers [{type:url, url:'https://A-host:443@B-host/mcp', name:'t', authorization_token:'dummy-token-not-real'}] and the mcp-client beta. |
| --web and --mcp put third-party-retrievable content and every listed MCP server's whole token-bearing toolset in one Messages request, with no per-tool narrowing | `ask-claude:316` | Whether the model follows instructions in web_fetch/web_search results or in one MCP server's tool result to call a tool on a token-bearing MCP server is model behavior that cannot be observed offline. | Done (v07, independent): the replayed one-shot --web --mcp -f request carries tools [web_search, web_fetch, {type: mcp_toolset, mcp_server_name: mcp-example-com}] with no restriction, mcp_servers with authorization_token tok_V07_DUMMY, and the -f contents ahead of the prompt; with a pause_turn replay one prompt produced 9 requests with the same tools and no user step (agents/v07/artifacts/v07-compose.out). | Owner-observed, non-destructive: run an MCP server the owner controls exposing a harmless write tool (e.g. |
| --memory access suffix other than exact ':ro' silently attaches the memory store without read_only | `lib/session-create.jq:33` | The provider default access mode for a memory_store resource sent without an access field (read_write vs read_only) is not observable offline; the issue has security impact only if that default is writable | Done: v08 sandbox replay (scratch mem-check.sh, output mem-check.out) with usage_memory='memstore_a:ro' and 'memstore_b:RO memstore_c:readonly memstore_d:read_only memstore_e: memstore_f:rw memstore_g:ro:x' and ASK_CLAUDE_REQUEST_LOG; only the exact 'ro' second field yields access:read_only, and session_prereqs raises no error. | Owner-observed, non-destructive, in the owner's own workspace on a dummy memory store with no real content: create a session with the store attached and no access field, then read back the session's resources (GET the session) and record the effective access mode. |

## Hardening notes

These are source facts that cross no demonstrated boundary. Hunters recorded them, and they are not findings.

- check-effver prints commit-controlled subject, VERSION and CHANGELOG.md heading raw (tools/effver/check-effver:82,97,101,124). ESC/OSC 52/BEL bytes reach the developer terminal, and a newline in VERSION lets it start forged lines (including line-start GitHub workflow commands) in the CI log. No boundary is crossed because git log/git show emit the same bytes and a PR author already controls the workflow. Suggested fix: pass printed values through a C0/C1 stripper (e.g. tr -d '\000-\037\177' or printf %q) before printing.
- effver.yml:46-52 tees effort-floor output, including the last version commit's subject (effver.just:62), into GITHUB_STEP_SUMMARY inside a ``` fence that a subject can close. This is cosmetic summary spoofing only.
- actions/checkout@v7 (all workflows) and Swatinem/rust-cache@v2 (rust.yml:54) are mutable tags. entity-diff.yml runs checkout@v7 with a pull-requests: write token persisted in .git/config. Suggested fix: pin to commit SHAs like mise-action and sem/action, and set persist-credentials: false where the token is not needed.
- `just actionlint` (justfile:34) fetches actionlint@1.7.12 through mise x, outside mise.toml/mise.lock, so it has no checksum pin. It runs with contents: read only.
- lib/event.jq num passes any JSON number and jq 1.7 (sandbox) emits unmodified literals verbatim, so an endpoint can write an arbitrarily long digit string (e.g. request text encoded as decimal bytes) into input_tokens/output_tokens/cache_read/cache_write; observed with a 47-digit literal. cents likewise has no length cap on list_cost. This contradicts the README sentence that only a single identifier-like word passes, but crosses no boundary (same endpoint already holds the data; no consumer). Suggested fix: num as `if type == "number" and . >= 0 and . < 1e15 then floor else null end` (arithmetic canonicalises the literal) and cap cents to e.g. {1,15} digits. The pinned jq 1.8.2 literal-preservation behaviour was not run here (sandbox has jq 1.7).
- advisor_used (lib/request.jq:58-64) trusts server_tool_use blocks in endpoint-written history; a forged history can only lower the client-side max_uses cap. Harmless today because the cap is a cost guard against the same endpoint, but it should not be reused as a security control.
- The Messages transport (ask-claude:316), the session stream (488), file upload (985) and file download (1035) call curl without -g, while ma_call uses -g. Braces or brackets in the BASE_URL path expand into several requests: http://localhost:9/{a,b} produced two connection attempts (base-url-gate.out). Each request still goes to the same host. Add -g to every curl call.
- The http branch of the --base-url regex (ask-claude:101), (/.*)?$, accepts an embedded newline because '.' matches newline in bash ERE. curl rejects that URL today. Use [^[:space:][:cntrl:]]* so the gate does not depend on curl.
- ASK_CLAUDE_REQUEST_LOG (ask-claude:308, 453) writes full bodies, including authorization_token and GITHUB_TOKEN. umask makes a new log 0600, but appending to an existing file keeps that file's mode. Consider redacting token fields, or documenting the variable as test-only.
- The -S system prompt reaches jq argv on every request (ask-claude:346). It is already on ask-claude's own argv, so the exposure does not change; reading it from stdin or env would match the stated secrets-off-argv design.
- ask-claude:145-152 run chmod 700 before the group/other write-bit test, so for a directory the user owns the test can never fail. A state dir that was writable by others at some earlier time is silently fixed and accepted together with anything planted in it while it was writable (local check 2: planted.json kept). Suggestion: test the mode before chmod and refuse, or ask the user to inspect, when it was writable by others.
- ask-claude:150 reads only the POSIX mode bits from ls -ldn. On macOS, an ACL inherited from a parent with directory_inherit can grant another account write; chmod 700 does not strip it and the cut -c6,9 test ignores it. Unverified: the sandbox is Linux. On macOS, check with ls -lde or refuse when ls shows '+'.
- ask-claude:1023-1040 (agent files) checks $name and later runs mv -f by path. If another account that can write the cwd swaps $name for a symlink to a directory between the check and the mv, both GNU and BSD mv move the temp file into that directory as .<name>.XXXXXX. A file with a random hidden name lands where the user can write. Low impact, and it needs a cwd writable by others. mv -T (GNU) or a re-check right before rename would close it.
- ask-claude:921-922 session_find echoes an unmatched user argument beginning sesn_ unvalidated, and it reaches rm -f "$SESSIONS/$1" (1068) after a successful archive. '/' and glob characters are refused, so the value stays a single component inside sessions/. User-supplied input; no boundary crossed.
- ask-claude:605 `sh -c "$cmd"` passes the whole environment to the local MCP server, including ANTHROPIC_API_KEY, GITHUB_TOKEN and ASK_CLAUDE_MCP_TOKEN_* (seen locally in check1-reap-fds-env.txt). The MCP reference clients pass a small default environment plus configured variables. This is not a boundary finding: the server runs as the same uid and can read the parent's environment anyway (/proc/<ppid>/environ on Linux, KERN_PROCARGS2 on macOS). Suggested change: start the server with `env -u ANTHROPIC_API_KEY -u GITHUB_TOKEN ...` (and the per-host MCP token variables), or with an allowlisted environment, as defense in depth against accidental secret logging by third-party servers.
- ask-claude:634-655 stale process-group handle: bash reaps the background job when it exits, so after an early server exit LOCAL_MCP_PID can be reused, and local_mcp_alive plus `kill -- -$LOCAL_MCP_PID` could then signal an unrelated same-user process group at exit. The code comment's zombie premise is wrong (verified locally). Suggested change: in local_mcp_stop, first check `jobs -p`, or check whether `wait -n`/`kill -0` still sees the job as running. Alternatively, record the server's start time (`ps -o lstart= -p`) at start and confirm it before signalling. Regression test: a server that exits after the handshake, then a ps shim reporting that pgid, and assert that stop sends no signal.
- ask-claude:597-598 the FIFO directory leaks when mkfifo fails, because local_mcp_stop returns early while LOCAL_MCP_PID is empty. Suggested change: in local_mcp_stop, remove LOCAL_MCP_DIR whenever it is set, independent of the PID.
- ask-claude:605 the server's stderr is the user's terminal, with no `safe` filtering. Not a boundary: the same-uid server shares the controlling terminal and can open /dev/tty itself.
- ask-claude:504 stops display on a substring match of '"type":"session.status_idle"' anywhere in a line, including inside a tool_use input object, while the fold parses the whole log. Locally this hides later agent.message text and tool names from the terminal (stream-check.out). Suggestion: decide the stop on the parsed top-level .type, e.g. a jq filter that emits a sentinel, instead of sed on raw text.
- ask-claude:828: the 8-round custom-tool cap increments only when a round has no agent.tool_use asks. An endpoint or model that pairs one always_ask server tool call with custom calls in every round keeps rounds at 0, so local MCP calls continue past the cap under an 'always' grant or ASK_CLAUDE_CONFIRM=allow. Each call is still confirmed or already allowed, so this weakens a runaway limit, not authorization. It is adjacent to the peer-owned confirmation units.
- --repo forwards the ambient GITHUB_TOKEN, a variable many developers set to a broad-scope PAT for gh and other tools, into the session body (lib/session-create.jq:29). The use is intentional and documented, but least authority would favor a dedicated variable (e.g. ASK_CLAUDE_GITHUB_TOKEN) or documentation recommending a fine-grained, single-repo, read-only token.
- --repo accepts '.' or '..' as the repository name (bash ask-claude:1094 and jq lib/session-create.jq:25 both allow [^/@]+), producing mount_path /workspace/.. (local replay). How the provider handles it is unknown and the value is user-controlled. Suggestion: reject names that are '.' or '..', or that start with '.'.
- bash ERE (ask-claude:1094) and Oniguruma (lib/session-create.jq:25) disagree on newlines in the branch: bash accepts 'o/n@main<LF>x', the jq capture fails, and repo_resource yields empty, so the session is created without the requested repo and with no error (local replay). Fail closed: check the capture result and die.
- ask-claude:1085 and :1088 iterate ${usage_vault} and ${usage_memory} unquoted without set -f, so IFS splitting (tab, newline) and pathname globbing apply in bash, while jq's words splits only on spaces. The client-side format checks can validate different values than the ones sent (local: --vault '[v]lt_x' with a cwd file vlt_x passes, and the literal is sent). Suggestion: use set -f and read -ra with IFS=' ' to match jq's split.
- safe (ask-claude:157, lib/stream.jq:29-32, lib/session.jq:12-15, lib/session-history.jq:10-13) strips C0/C1/DEL but not bidi overrides (U+202A-202E, U+2066-2069) or zero-width code points, and jq's tojson leaves them raw. In the y/N/a prompt (ask-claude:537), an endpoint- or model-chosen input string could visually reorder on bidi-rendering terminals. This is renderer-dependent and belongs to the peer confirmation-binding unit; suggested fix: drop or escape those code points in safe.
- ask-claude:741 derives name with $(jq -r '.name | tostring'), which strips trailing newlines, so a custom_tool_use name 'tool\n' passes local_mcp_has as 'tool' and is shown as 'local tool'. local_mcp_call (ask-claude:677) then sends the raw .name 'tool\n' to the server. The allowlist and confirmation should be checked against the exact value sent, e.g. compare inside one jq call and refuse non-string names. No impact was observed: a conforming server rejects the unknown name.
- agent tools (ask-claude:1170-1172) exits 0 even when jq fails, for example when a server's inputSchema is a string or array and del(."$schema") errors. The user then pastes partial or empty output into the agent config without a failure signal.
- The --local-mcp server's stderr is inherited and reaches the terminal raw (ask-claude:604). This is not a boundary, because the user picked the command and it runs with the user's uid. A server that relays remote content on stderr would bypass safe.
- Forwarded local MCP results (ask-claude:693-712) enter the remote agent's context, where server-side tools under an always_allow policy (e.g. web_fetch) can send them out with no client confirmation. The client cannot enforce server tool policy, so this is a session-configuration design point, not a client defect.
- The 'always' answer (ask-claude:536) creates one process-wide flag (documented at ask-claude:138) that also approves later local MCP calls and any tool name or argument. An 'a' given at a prompt for a remote sandbox tool (agent.tool_use bash) silently pre-approves local tools that run with the user's uid. Suggestion: scope the grant to (local|remote, tool name), or exclude 'local' calls from the always answer.
- session_confirm treats any existing path at $STATE/.always.$$ as a grant (`-e`, ask-claude:523), and the startup `rm -f` (:1113) cannot remove a directory of that name. This is reachable only by the same uid, because the state-dir gate refuses other owners and writers. Testing `-f` and creating the grant with noclobber would remove the edge case.
- session_answer derives the allowlist name with $(jq -r '.name | tostring') (ask-claude:741), which strips trailing newlines, while local_mcp_call sends the raw .name (:677). A name of 'tool\n' passes local_mcp_has as 'tool' but reaches the server as a different name. There is no escalation, because the server sees an unknown name, but the check and the use differ. Suggestion: compare inside jq on the same object.

## Positive patterns

- `--local-mcp`:
  - FIFOs live in a `mktemp -d` directory with mode 0700, removed on stop.
  - The server is forked before ask-claude opens its fds.
  - Shutdown escalates EOF → SIGTERM → SIGKILL against the whole group. This was verified against a server that ignores both.
  - The approved object is the executed object (`$use .name`/`.input`), gated by an exact-name allowlist from the server's own `tools/list`.
- The event log builds each line from a fixed object literal with identifier, hostname and number filters, and nothing reads it back.
- The session id from the API is shape-checked before it becomes a path. Downloads use `mktemp` + `mv` and refuse symlinks.
- CI uses `pull_request` only, `contents: read`, and no secrets. The one write token skips forks. mise tools are checksum-locked.
- Secrets stay off argv. The `--base-url` gate refuses userinfo, and curl never follows redirects.

## Coverage

candidate: 8, covered: 13, deferred: 1, out_of_scope: 6 (of 28 units).

- **Post-wave critic:** accepted 1 missing unit (the `effver.just` bump recipe and `bump-effver` VERSION sink). It is recorded as deferred because no second wave was funded. It reassigned nothing.
- **Final-clean critic:** no further missing units or reassignments.
- **Phase 5:** 8 records, each checked by a fresh verifier; 7 verified as written, 1 non-material replacement applied directly (f03: tojson escapes DEL, so only C1 is dropped from the prompt). No verdict, severity or root cause changed, so no further verifier was needed.

Because a critic-accepted unit is deferred, this run makes **no clean-coverage claim**, even within its scope. Rejected records: 0.

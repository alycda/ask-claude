# Changelog

Versions follow [EffVer](https://jacobtomlinson.dev/effver/):
`MACRO.MESO.MICRO`, where the number that changes says how much effort it
takes to adopt the change. Macro: a large effort. Meso: some effort. Micro:
no effort. In `0.x` the format is `0.MACRO.MICRO`.

Each heading is `## <version> (<effort>) - <date>`. `just bump <effort>`
adds one, and `just check-effver` checks it against `VERSION`.
`just effort-floor` names the lowest effort a release can claim, from a
`usage diff` of the `#USAGE` spec since the last version.

## 0.3.1 (meso) - 2026-11-10

- `--web` sends `web_search_20260318` and `web_fetch_20260318`, the
  dynamic-filtering versions, in place of `web_search_20250305` and
  `web_fetch_20250910`: the model filters results in code before they reach
  its context. A model id older than Claude 4.6 keeps the basic versions,
  because the newer ones are a 400 there. Aliases (`haiku`, `opus`, ...)
  get the dynamic versions. Without `-s`, a `pause_turn` continuation sends
  back the container the paused round ran in, so filtering that was in
  progress resumes there. A new turn starts without one.

- `--mcp-tool <host>:<tool>` (repeatable) limits an `--mcp` server to the
  tools named. Its `mcp_toolset` goes out as an allowlist
  (`default_config: {enabled: false}`, the named tools enabled in
  `configs`). A server no entry names keeps all its tools. An entry whose
  host is no `--mcp` server, or that is not `<host>:<tool>`, exits 2
  before any request.
- Each event-log line has two more keys, at the end:
  - `tools`: what the request let the model reach, failed requests
    included.
  - `tool_calls`: web searches and fetches from `usage.server_tool_use`,
    MCP calls counted from the reply.

  Both are `null` for a session turn. The line format stays `v: 1`: the
  existing keys and their order are unchanged.
- Adopting needs:
  - **An organization under Zero Data Retention using `--web`:** the
    dynamic-filtering versions are not ZDR-eligible. Pass an older model
    with `-m` to get the basic versions; this version has no flag to keep
    them on a 4.6+ model.
  - **A reader that compares whole event lines byte for byte:** it sees
    the two new keys.

## 0.3.0 (macro) - 2026-11-07

- `agent --local-mcp <command>` (or `ASK_CLAUDE_LOCAL_MCP`) runs a stdio MCP
  server on this machine and answers the session's custom tool calls with
  it. Each call goes through the same tool confirmation as the agent's
  other tools and shows as `local <name>`. Text and embedded text
  resources go back to the session; other content is named, not sent.
  Custom-tool rounds are capped at 8 a turn. The server starts in its own
  process group, and the whole group is stopped when ask-claude exits,
  with SIGKILL if SIGTERM fails.
- `agent tools --local-mcp <command>` prints the server's tools as custom
  tool definitions to paste into the agent, with the top-level `$schema`
  dropped. It needs no API key.
- `--mcp` tokens are per server: a server gets `ASK_CLAUDE_MCP_TOKEN_<HOST>`.
  The shared `ASK_CLAUDE_MCP_TOKEN` still works with one `--mcp`, and is
  refused with more than one.
- The state directory is refused, before any request, when it is a
  symlink, when `chmod 700` fails, or when another account owns it.
- `agent files` aligns its list without `column`, and exits 1 when it
  cannot print one.
- Security audit run 2 (`docs/security-audit/`) replaces run 1's report.
  It confirmed 3 low findings, which are not fixed in this version.
- Adopting needs:
  - **More than one `--mcp` server with the shared `ASK_CLAUDE_MCP_TOKEN`:**
    move the token to one `ASK_CLAUDE_MCP_TOKEN_<HOST>` per server.
  - **A one-shot prompt starting with the word `tools` under `agent`:**
    it now runs `agent tools`.
  - **A state directory that is a symlink or another account's:** point
    `XDG_STATE_HOME` at a directory of your own.

## 0.2.7 (micro) - 2026-11-07

- An event log: every Messages request and every session turn appends one
  JSON line to `events.jsonl` in the state directory, with model, tokens,
  cache reads and writes, advisor usage, seconds, stop reason and host. It
  never holds prompt, reply or error text. A `pause_turn` round is its own
  line, and a session turn is one line carrying the session's usage so far. The README's
  "Events on disk" has the fields and five `jq` queries: tokens per chat,
  cache hit rate, seconds by model, refusals and advisor spend.
- Adopting needs no change: the log is on by default and private like the
  history. `ASK_CLAUDE_EVENTS=off` turns it off.
- Known gap: Ctrl-C during a Messages request leaves no line.

## 0.2.6 (micro) - 2026-11-07

- `--advisor <model>` lets the reply consult another model mid-turn, through
  the advisor tool (beta): at most 3 calls a turn, billed at the advisor's
  rates. The stream shows `(advisor) `, or `(advisor error: <code>) `, and
  the footer adds `advisor=<model> in=… out=…`. A chat that used an advisor
  resumes without the flag.

## 0.2.5 (micro) - 2026-11-06

- Ten fixes from the security audit (`docs/security-audit/`): `--base-url`
  checks the parsed host, so `http://localhost:@example.com` no longer sends
  the key to example.com; the `always` answer to a tool confirmation ends with
  its process instead of living on as a file a later process could inherit;
  every Managed Agents value the script prints goes through `safe`; the
  confirmation prompt shows the tool input encoded once; request text reaches
  jq on stdin, never on a command line; the API session id is validated
  before it names a file; `agent files` writes a fresh temp file, never
  through a symlink; the session fold has explicit defaults, so jq 1.8 folds
  a turn; the recorder redacts ids as values only, and more kinds of them;
  `check-effver` validates the parent `VERSION` before arithmetic.
- The session fixtures are live recordings, ids redacted; the harness says
  which streams are recorded and which are derived, and names a missing one.
- The gates report zero failures with their exclusions in the command:
  `just unit-tests` skips the terminal-bound test, which `just pty-test` runs
  alone; the script-bound tests are tagged for the Rust suite to skip.
  `usage` is pinned at 7.3.0.
- Adopting needs no change from 0.2.4: a `--base-url` with user info before
  the host and an `always` grant that outlived its process were bugs.

## 0.2.4 (micro) - 2026-11-05

- `--web` lets the model search and fetch the web; `--mcp <https url>` lets it
  call an MCP server (`ASK_CLAUDE_MCP_TOKEN` for its auth). Both work with
  `-s`, in a one shot and in the chat, and the stream shows what is being
  used. A web search costs $10 per 1,000; everything else is tokens.
- Adopting needs no change from 0.2.3.

## 0.2.3 (micro) - 2026-11-05

- `ask-claude agent budget <session> <cents>` changes a session's cap (`0`
  removes it, for good), `agent archive <session>` archives an idle one,
  `agent trace <session>` prints its page in the Console
  (`ASK_CLAUDE_WORKSPACE` when the key is not in the Default workspace).
  Nothing the spec declares is refused any more.

## 0.2.2 (micro) - 2026-11-05

- `agent --vault <vlt_id>`, `--repo owner/name[@branch]` (with `GITHUB_TOKEN`)
  and `--memory <memstore_id>[:ro]` attach credentials, a repository and
  memory stores to the session; the README says what to make in the Console
  for each.
- Still refused with exit 2: `agent budget`, `agent archive`, `agent trace`.

## 0.2.1 (micro) - 2026-11-05

- `-f FILE` with `agent` uploads the file and the agent finds it at
  `/mnt/session/uploads/<name>`; `ask-claude agent files <session>` lists what
  a session holds and downloads the named ones into the current directory
  (`--force` to overwrite).
- Still refused with exit 2: `agent budget`, `agent archive`, `agent trace`.

## 0.2.0 (macro) - 2026-11-05

- `ask-claude agent "question"` runs a prompt as a Managed Agents session,
  and `ask-claude agent` with no prompt a chat on one (`/new`, `/history`,
  `/quit`); `agent list` and `agent resume <session>` work from the API, which
  keeps the transcript. The reply streams, the usage line shows the session
  id, tokens and cost at list price. Every session gets a budget (`--budget`,
  100 cents unless told otherwise, `0` for none). A tool confirmation is
  asked at the terminal and denied without one (`ASK_CLAUDE_CONFIRM`);
  custom tools on the agent get an error result.
- Declared but not built yet, and refused with exit 2: `-f` with `agent`,
  `agent files`, `agent budget`, `agent archive`, `agent trace`.
- Breaking: the word `agent` as a one-shot prompt now opens the command
  (`ask-claude -- agent` keeps the old meaning).
- To adopt: make an environment and an agent in the Console
  (platform.claude.com), put their ids in `ASK_CLAUDE_ENVIRONMENT` and
  `ASK_CLAUDE_AGENT` (`just environments`, `just agents`), and read the Agents
  section of the README for what a session costs.

## 0.1.4 (micro) - 2026-11-05

- The bash `ask-claude` now does everything `claude.sh` did (one shot,
  chat loop, history, skills), with the same request shape and the same
  goldens. Adopting needs no change from 0.1.3.
- `--base-url` sets the API root for `-p anthropic` (https, or http on
  localhost), for a stub or a proxy. `-p openai` stays refused in bash:
  the OpenAI shape, Venice included, is the Rust port's.

## 0.1.3 (micro) - 2026-11-05

- `-s pptx,skill_abc@skver_1` runs the turn with those skills in
  Anthropic's code-execution sandbox; the stream shows `(running code)`
  and `(result rc=N)` while nothing prints. A chat keeps its container
  beside its file (`<chat>.json.container`), so `resume -s ...` reuses
  the files from earlier turns.
- Still refused with exit 2: `-p openai`, `--base-url`.

## 0.1.2 (micro) - 2026-11-05

- `ask-claude` with no prompt opens a chat: each line is a turn, `/new`
  starts another chat, `/history` prints this one, `/quit` exits and
  prints the command to resume it. A multi-line paste is one turn under
  bash 5.1 or newer (macOS `/bin/bash` 3.2 sends on the first newline).
- Still refused with exit 2: `-s`, `-p openai`, `--base-url`.

## 0.1.1 (micro) - 2026-11-05

- `ask-claude list` shows the chats on disk, newest first, and
  `ask-claude resume [chat]` continues one by name or prefix (the newest
  with turns when omitted), printing it first. One-shot replies were
  already saved; now they can be picked up. `list` needs no key.
- Request flags (`-m`, `-e`, `-S`, `--max-tokens`) also apply to `resume`.
- Still refused with exit 2: the chat loop, `-s`, `-p openai`, `--base-url`.

## 0.1.0 (macro) - 2026-11-05

- First version: one-shot. `ask-claude "question"` streams the reply,
  prints a usage line on stderr, and saves both turns. `-f` prepends a
  file; `-m`, `-e`, `-S` and `--max-tokens` shape the request; `-V`
  prints the version.
- Declared but not built yet, and refused with exit 2: `list`, `resume`,
  the chat loop, `-s`, `-p` and `--base-url`.
- To adopt: install `curl`, `jq` and `usage` (or `mise install`), set
  `ANTHROPIC_API_KEY`, put `ask-claude` on `PATH`.

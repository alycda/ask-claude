# ask-claude

Talk to Claude from a shell with nothing but `curl` and `jq`. One script,
one request shape, streaming always on.

```sh
ask-claude "one question"           one shot, prints the answer
ask-claude                          chat loop
ask-claude -f notes.md "summarize"  file contents + prompt, one shot
ask-claude list                     chats on disk, newest first
ask-claude resume [chat]            continue a chat (name or prefix)
ask-claude --help                   every flag
```

`--web` lets the model search and fetch the web (server tools; a search
costs $10 per 1,000, a fetch nothing beyond tokens). On Claude 4.6 and
later (every alias, and the default) these are the dynamic-filtering
versions, `web_search_20260318` and `web_fetch_20260318`: the model runs
code that filters results before they reach its context, and the stream
shows `(running code) ` while it does. They are not eligible for Zero Data
Retention. An older model id (`claude-haiku-4-5`, `claude-sonnet-4-5-…`,
the 3.x family) gets the basic versions, which are. `--mcp <https url>`
(repeatable) lets it call an MCP server through the MCP connector. A
server's authorization token is `ASK_CLAUDE_MCP_TOKEN_<HOST>`, the host
upper-cased with every other character an underscore
(`ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM` for `https://mcp.example.com/mcp`),
and each server gets only its own. `ASK_CLAUDE_MCP_TOKEN` is for a lone
`--mcp`: with several, it would reach every server's operator, so the
script exits 2 and names each server's variable. Both work with
`-s`, in the one-shot and in the chat; the stream shows `(web_search) `,
`(web_fetch) ` and `(mcp <server>) ` while the model uses them.

`--mcp-tool <host>:<tool>` (repeatable) limits that `--mcp` server to the
tools named: its toolset goes out as an allowlist, every tool off but
those. Anthropic calls an MCP server from its side, so ask-claude cannot
ask before a call; what the model passes to a tool reaches that server's
operator, and that can include `-f` file contents. The allowlist decides
before the reply which tools the model can call at all. A server no
`--mcp-tool` names keeps all its tools, as before. An entry whose host is
no `--mcp` server exits 2: it would match nothing, and the server it was
meant for would keep every tool.

```sh
ask-claude --mcp https://mcp.deepwiki.com/mcp \
  --mcp-tool mcp.deepwiki.com:read_wiki_structure \
  --mcp-tool mcp.deepwiki.com:ask_question "how does tokio schedule tasks?"
```

`--advisor <model>` lets the model consult another model mid-reply (the
advisor tool, a server tool in beta): at most 3 calls a turn, each billed at
the advisor model's rates and shown in the footer as `advisor=<model> in=…
out=…`. The advisor must be at least as capable as `-m`: with the default
Opus 5.5 that is Opus 5.5 itself, or Fable 5.1; a Sonnet or Haiku reply can
take Opus as its advisor. Most advisors return their advice encrypted, so the
stream shows `(advisor) ` and the reply, not the advice. A chat that used an
advisor can be resumed without `--advisor`.

In the chat loop, `/new` starts a fresh chat, `/history` prints the current
one, and `/quit` exits and prints the command to resume it.

## Environment

| Variable | Use |
| --- | --- |
| `ANTHROPIC_API_KEY` | required; there is no config-file fallback |
| `XDG_STATE_HOME` | chats live in `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude/` |
| `ASK_CLAUDE_REPLAY` | testing: read this recorded `.sse` file instead of calling the API |
| `ASK_CLAUDE_REQUEST_LOG` | testing: append each request body to this file |
| `ASK_CLAUDE_BETA_LOG` | testing: append each request's `anthropic-beta` header value to this file |
| `ASK_CLAUDE_MCP_TOKEN` | authorization token for a lone `--mcp` server; refused with several |
| `ASK_CLAUDE_MCP_TOKEN_<HOST>` | authorization token for the `--mcp` server at that host only |
| `ASK_CLAUDE_EVENTS` | `off` turns off the event log (see "Events on disk") |

Nothing else is read.

## Chats on disk

One JSON array of turns per chat, named by its start time
(`20260102T150405.json`). A chat that used skills keeps its container id
beside it in `<name>.json.container`, so the next turn reuses the container.
An empty chat leaves no file.

The state directory must be a real directory you own, mode 700. The script
sets the mode and exits 2 when the directory is a symlink, cannot be created
or made private, or is owned by another account.

## Events on disk

Every Messages request and every Managed Agents session turn appends one
JSON line to `events.jsonl` in the same directory. It holds what the footer
shows (tokens, timing and outcome), plus which tools the request enabled
and how often the reply used them. It never holds prompt or reply text, file
contents, tool inputs, error messages or the key. Strings that come from the
API (model, error type, stop category, session id) are kept only when they
look like identifiers, and anything else is written as `"unknown"`. Token
counts that are not numbers are written as `null`. That bounds what an
endpoint can slip in, but a single word that looks like an identifier still
passes. The file is private (mode 600) and only ever appended to.
`ASK_CLAUDE_EVENTS=off` turns it off.

A `pause_turn` continuation is its own request, so it gets its own line. A
Managed Agents session gets one line per turn, and its usage is the
session's so far (`usage_scope: "session"`). To total a session, take the
last line or the maximum, not the sum.
A session that fails to start gets one error line with `chat: null`.

One known gap: Ctrl-C during a Messages request ends the script before the
line is written, so an interrupted Messages request leaves no line. A session
turn interrupted with Ctrl-C writes `interrupted`.

| Field | Value |
| --- | --- |
| `v`, `ask_claude` | line format (1) and script version |
| `ts` | UTC, ISO 8601, to the second |
| `kind` | `messages` or `session` |
| `chat` | the chat's name as `list` shows it, the session id, or `null` for a session that never started |
| `host` | the API host and port from `--base-url`, no path |
| `model` | the replying model (`null` for sessions) |
| `round` | `pause_turn` round from 0, or custom-tool rounds answered |
| `seconds` | whole seconds, as in the footer |
| `stop_reason`, `stop_category` | how the reply ended, and the refusal category |
| `usage_scope` | `request`, or `session` for cumulative session usage |
| `input_tokens`, `output_tokens`, `cache_read`, `cache_write` | tokens |
| `list_cost` | sessions only: the API's cost so far, in cents, as a string |
| `advisor` | `{model, calls, input_tokens, output_tokens}`, or `null` |
| `error` | `null`, the API's error type, `unknown` (an error without one), or `no_response`, `parse`, `unreadable`; sessions also `interrupted` (Ctrl-C) and `unanswerable` |
| `tools` | what the request let the model reach: each server tool by its versioned type (`web_search_20260318` and `web_fetch_20260318`, or the basic `web_search_20250305` and `web_fetch_20250910`; `code_execution_20260521`; `advisor_20260301`), `mcp:<server>` for a server with all its tools, one `mcp:<server>:<tool>` per tool of an `--mcp-tool` allowlist; `null` for no tools and for sessions (the agent's tools live on the agent). Kept on a failed request: the tool set was decided before it. |
| `tool_calls` | `{web_search, web_fetch, mcp}`: searches and fetches as the API's `usage.server_tool_use` counts them, MCP calls as `mcp_tool_use` blocks in the reply; `null` when no reply arrived (`no_response`, `unreadable`, `parse`, an HTTP error body) and for sessions. A reply that fails mid-stream still counts the calls it made: they ran and were billed |

Six questions, each answered with `jq` alone:

```sh
EVENTS="${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude/events.jsonl"
```

Tokens per chat (sessions take their last total):

```sh
jq -s 'group_by(.chat) | map(.[0].usage_scope as $scope
  | def total(f): map(f // 0) | if $scope == "session" then max else add end;
  {chat: .[0].chat, kind: .[0].kind,
    input_tokens: total(.input_tokens), output_tokens: total(.output_tokens),
    cache_read: total(.cache_read), cache_write: total(.cache_write)})' "$EVENTS"
```

Cache hit rate by model (the share of input read from the cache):

```sh
jq -s 'map(select(.kind == "messages" and .error == null)) | group_by(.model) | map({model: .[0].model,
  cache_hit: ((map(.cache_read) | add) / ([map(.input_tokens + .cache_read + .cache_write) | add, 1] | max))})' "$EVENTS"
```

Seconds by model:

```sh
jq -s 'map(select(.kind == "messages" and .error == null)) | group_by(.model) | map({model: .[0].model,
  requests: length, mean_seconds: (map(.seconds) | add / length), max_seconds: (map(.seconds) | max)})' "$EVENTS"
```

Refusals by category:

```sh
jq -s 'map(select(.stop_reason == "refusal")) | group_by(.stop_category)
  | map({category: .[0].stop_category, refusals: length})' "$EVENTS"
```

Requests that could reach an MCP server, and the calls they made, by tool set:

```sh
jq -s 'map(select(.kind == "messages" and any(.tools[]?; startswith("mcp:"))))
  | group_by(.tools) | map({tools: .[0].tools, requests: length,
    mcp_calls: (map(.tool_calls.mcp // 0) | add)})' "$EVENTS"
```

Advisor spend by advisor model:

```sh
jq -s 'map(select(.advisor != null)) | group_by(.advisor.model) | map({advisor: .[0].advisor.model,
  calls: (map(.advisor.calls) | add), input_tokens: (map(.advisor.input_tokens) | add),
  output_tokens: (map(.advisor.output_tokens) | add)})' "$EVENTS"
```

## Agents

`ask-claude agent` runs a prompt, or a chat, as a [Managed Agents](https://platform.claude.com/docs/en/managed-agents/overview)
session: an Anthropic-run sandbox with network, packages, web tools and
MCP servers, as the agent you made allows. The API keeps the transcript.

```
ask-claude agent "one question"          one shot in a new session
ask-claude agent                         chat; /new, /history, /quit
ask-claude agent list                    sessions on the account, newest first
ask-claude agent resume <sesn|prefix>    continue one
ask-claude agent files <sesn> [name]... what the agent wrote; download the named ones
ask-claude agent budget <sesn> <cents>  change the cap (0 removes it, for good)
ask-claude agent archive <sesn>          archive an idle session
ask-claude agent trace <sesn>            its page in the Console
ask-claude agent tools --local-mcp <cmd> that server's tools as custom tool definitions
```

Once, in the Console (platform.claude.com): an **environment** (start with
limited networking, add `allowed_hosts` for what the job needs) and an
**agent** (model, system prompt, which tools; set bash to *ask* unless you
trust it). `just environments` and `just agents` print their ids. Put them in
`.env` or pass `--environment` and `--agent`:

| Variable | Use |
|---|---|
| `ASK_CLAUDE_AGENT` | the agent id, `--agent` overrides |
| `ASK_CLAUDE_ENVIRONMENT` | the environment id, `--environment` overrides |
| `ASK_CLAUDE_CONFIRM` | `ask` (default), `allow`, or `deny` for tool confirmations |
| `ASK_CLAUDE_LOCAL_MCP` | the `--local-mcp` command, `--local-mcp` overrides |
| `ASK_CLAUDE_LOCAL_MCP_TIMEOUT` | seconds to wait for one local tool call, 120 by default |

Every session gets a budget, 100 cents at list price unless `--budget` says
otherwise (`0` for none). Session time bills at $0.08 per active hour on top
of tokens, and web searches at $10 per 1,000; the usage line shows cost so far. When the agent wants to run a
tool its policy marks *ask*, ask-claude asks you (`y`, `n`, `a` for the rest
of the run) and denies when there is no terminal. A custom tool on the
agent is answered with an error unless `--local-mcp` runs it (below).

### Local tools: `--local-mcp`

An agent's MCP servers are called from Anthropic's side, so they must be
public https URLs. A custom tool is the other way round: the session pauses,
this machine runs the tool, and the result goes back. `--local-mcp <command>`
starts a stdio MCP server on this machine (with `sh -c`, once per session)
and answers each custom tool call that the server lists:

```sh
ask-claude agent tools --local-mcp 'tb --mcp' > tools.json   # once
# add the entries of tools.json to the agent's tools, in the Console or with
# POST /v1/agents/<id>; the names must match the server's
ask-claude agent --local-mcp 'tb --mcp' "what is due this week?"
```

Each call is a tool confirmation like any other, asked at the terminal as
`allow local <tool> <input>?`, denied when there is no terminal, and
answered by `ASK_CLAUDE_CONFIRM` when it is set. An allowed call runs on
this machine with your permissions, and its result goes to the API, so
the agent reads whatever the tool returns. Text content, and an embedded
text resource (how `github-mcp-server` returns a file), go back as text;
binary resources, resource links and images are named, not sent. A tool the server
does not list, a server that exits, an MCP error, or a call that takes longer
than `ASK_CLAUDE_LOCAL_MCP_TIMEOUT` is an error result, and the turn goes on.
A server that fails the handshake ends the run with exit 2 before any
request. The server's stderr is yours; it is not captured.

What else a session can carry, each made once in the Console:

- `--vault <vlt_id>` (repeatable): credentials for MCP servers the agent
  declares, or secrets substituted into the agent's outbound requests. The
  environment's `allowed_hosts` must include the credential's host.
- `--repo owner/name[@branch]`: a GitHub repository mounted at
  `/workspace/<name>`, read with `GITHUB_TOKEN` from the environment.
- `--memory <memstore_id>[:ro]` (repeatable, up to 8): memory stores mounted
  under `/mnt/memory/`, `:ro` for read-only.

`ask-claude agent trace <sesn>` prints a session's page in the Console
(`ASK_CLAUDE_WORKSPACE` when the key is not in the Default workspace). Ctrl-C
during a turn sends `user.interrupt`, so the session stops too. Sessions
started here
are listed in `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude/sessions/`;
`resume`, `files`, `budget`, `archive` and `trace` take a prefix of one.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | reply printed (a refusal also exits 0, with `[stopped: refusal <category>]` on stderr) |
| 1 | the API returned an error; the unanswered turn is not saved |
| 2 | bad input: no key, unreadable `-f` file, unknown chat for `resume` |

## Runtime dependencies

`curl`, `jq`, and [`usage`](https://usage.jdx.dev), which parses the
`#USAGE` block at the top of the script into flags, help, and shell
completions. `mise install` provides `jq` and `usage` at the versions
pinned in `mise.toml`; `curl` comes from the system.

bash 5.1 or newer, so a multi-line paste in the chat loop is one turn.

## Development

Every tool is pinned in `mise.toml`; there are two ways to get them.

**Devcontainer.** Open the folder in VS Code and run *Dev Containers:
Reopen in Container*, or `devcontainer up --workspace-folder .` from a
shell. The mise feature installs mise, and `postCreateCommand` runs
`mise install`, so `jq` and `usage` are on `PATH` once the container is
up. The mise binary comes from the feature's pinned release and does not
update itself; rebuild without the cache to get a newer one.

**On the host.** Install [mise](https://mise.jdx.dev), then:

```sh
mise install                                  # jq and usage, pinned
(umask 077; echo 'ANTHROPIC_API_KEY=sk-ant-...' > .env)   # mode 600; only just test reads it
just --list                                   # the map
```

`just skills` lists the custom skill ids on the account, for `-s`.
`just completions zsh --install` (or bash, fish) installs tab completion for
the spec, and says what to add to the shell's rc file.
`just test` makes one cheap live call per Anthropic contract,
`just test-agents` one session turn plus the lists (`ASK_CLAUDE_AGENT` and
`ASK_CLAUDE_ENVIRONMENT` in `.env`), and `just test-openai` does the same
for an OpenAI-compatible API (`OPENAI_BASE_URL`, `OPENAI_API_KEY` and
optionally `OPENAI_MODEL` in `.env`; Venice is just a base URL).

---

### Resources

- [Bash](https://learnxinyminutes.com/bash/)
- [Curl](https://curl.se/docs/tutorial.html)
- [Hurl](https://hurl.dev/)
- [JQ](https://learnxinyminutes.com/jq/)
- [Usage](https://usage.jdx.dev/)
- [Messages API](https://platform.claude.com/docs/en/build-with-claude/working-with-messages)
- [Managed Agents](https://platform.claude.com/docs/en/managed-agents/overview)
- [Justfile](https://github.com/casey/just)
- [Security Audit](https://github.com/cloudflare/security-audit-skill)
- [Shell Style Guide (Google)](https://google.github.io/styleguide/shellguide.html)

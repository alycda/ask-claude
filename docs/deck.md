---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.3.1

Talk to Claude from a shell with nothing but `curl` and `jq`.

Released 2026-11-13 · EffVer micro · `--mcp-tool` allowlists

<!--
This deck is rewritten at every release, so its history is the project's
history: `git log -p docs/deck.md`. Six slides, about five minutes.
The notes under each slide are for the presenter. Marp shows them in
presenter view; in any other reader they are HTML comments.
-->

---

## What it is

One script, two APIs, streaming always on.

```sh
ask-claude "one question"           # Messages API: one shot
ask-claude                          # chat loop
ask-claude --web "what changed?"    # the model may search and fetch
ask-claude --mcp <url> --mcp-tool <host>:<tool> "..."   # one MCP tool
ask-claude agent "question"         # a Managed Agents session
ask-claude agent --local-mcp <cmd>  # its custom tools run on this machine
```

Messages chats are JSON files under `$XDG_STATE_HOME/ask-claude/`.
A session's transcript lives with the API.

<!--
The split to explain: plain `ask-claude` talks to the Messages API and
keeps history on disk. `ask-claude agent` runs a Managed Agents session,
where the agent and the transcript are on Anthropic's side. `--advisor`
is still there; it left the slide to make room.
-->

---

## New since 0.3.0

- `--mcp-tool <host>:<tool>` limits an `--mcp` server to the tools
  named: its toolset goes out as an allowlist, every other tool off
- `--web` sends the dynamic-filtering tools, `web_search_20260318` and
  `web_fetch_20260318`: the model filters results in code before they
  reach its context
- Each event-log line gains `tools` (what the request let the model
  reach) and `tool_calls` (what it used)

<!--
Why the allowlist matters: Anthropic calls an MCP server from its side,
so ask-claude cannot ask before a call. The allowlist decides before the
reply which tools the model can call at all. The new web tools need
Claude 4.6 or later; an older model gets the API's 400. The event line
format stays `v: 1`: the two keys are appended, nothing moves.
-->

---

## How it is built

- A bash entry script, seven sourced `lib/*.bash` files and nine `jq`
  filters: 1,871 lines
- [`usage`](https://usage.jdx.dev), pinned at 7.3.0, reads the `#USAGE`
  block at the top of the script: flags, help, shell completions
- Every `feat:` and `fix:` commit follows a test that is red without it
- 307 bats tests replay recorded SSE and session streams
  (`ASK_CLAUDE_REPLAY`); 5 hurl files check the live API contracts
- GitHub Actions runs the gates on pull requests
- Versions are [EffVer](https://jacobtomlinson.dev/effver/): the number
  that changes says how much effort adopting takes

<!--
Line and test counts are `wc -l` and `grep -c '^@test'` on this
release's tree. The one file was split between 0.3.0 and this release:
common, config, chat, messages, local-mcp, agent-api and agent-turn,
one refactor commit each, behind a test that a missing lib file is
exit 2 and names the file.
-->

---

## Releases so far

<style scoped>table { font-size: 0.75em; }</style>

| Version | Effort | Date | What |
| --- | --- | --- | --- |
| 0.1.4 | micro | 2026-11-05 | `--base-url`, parity with `claude.sh` |
| 0.2.4 | micro | 2026-11-05 | Managed Agents sessions (0.2.0, macro); `--web`, `--mcp` |
| 0.2.5 | micro | 2026-11-06 | Ten fixes from the security audit |
| 0.2.6 | micro | 2026-11-06 | `--advisor`, the advisor tool on Messages |
| 0.2.7 | micro | 2026-11-06 | The event log, `events.jsonl` |
| 0.3.0 | macro | 2026-11-07 | `agent --local-mcp`, per-server MCP tokens |
| 0.3.1 | micro | 2026-11-13 | `--mcp-tool` allowlists, dynamic-filtering web tools |

<!--
One row per GitHub release. Versions between rows exist in CHANGELOG.md
but were not released on their own. The date is the tagged commit's;
CHANGELOG.md's heading can differ from it by a day or more.
-->

---

## Adopting

```sh
mise install                    # jq and usage, pinned; curl from the system
export ANTHROPIC_API_KEY=sk-ant-...
ask-claude "one question"       # with ask-claude on PATH
```

- Coming from 0.3.0: nothing to do
- `ask-claude` needs `lib/` beside it, as before; the directory now
  holds bash as well as `jq`
- A reader that compares whole event lines byte for byte sees two new
  keys at the end
- `--web` needs Claude 4.6 or later

<!--
Coming from before 0.3.0, the three macro changes still apply: per-server
MCP tokens, `agent tools` as a command, and the state directory checks.
`agent` needs an environment and an agent made in the Console.
-->

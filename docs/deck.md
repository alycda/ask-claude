---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.3.0

Talk to Claude from a shell with nothing but `curl` and `jq`.

Released 2026-11-07 · EffVer macro · local MCP servers for sessions

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
ask-claude --advisor <model> "..."  # consult a second model mid-turn
ask-claude agent "question"         # a Managed Agents session
ask-claude agent --local-mcp <cmd>  # its custom tools run on this machine
```

Messages chats are JSON files under `$XDG_STATE_HOME/ask-claude/`.
A session's transcript lives with the API.

<!--
The split to explain: plain `ask-claude` talks to the Messages API and
keeps history on disk. `ask-claude agent` runs a Managed Agents session,
where the agent and the transcript are on Anthropic's side. With
`--local-mcp`, the session's custom tool calls come back to a stdio MCP
server started here.
-->

---

## New since 0.2.7

- `agent --local-mcp <command>` answers a session's custom tool calls
  with a stdio MCP server on this machine, each call behind the same
  confirmation as the agent's other tools
- `agent tools --local-mcp <command>` prints the server's tools as
  definitions to paste into the agent; it needs no API key
- `--mcp` tokens are per server: `ASK_CLAUDE_MCP_TOKEN_<HOST>`
- The state directory is refused when it is a symlink, when
  `chmod 700` fails, or when another account owns it
- Security audit run 2: 3 low findings confirmed, not fixed here

<!--
Custom-tool rounds are capped at 8 a turn. The server starts in its own
process group and the group is stopped when ask-claude exits, SIGKILL
if SIGTERM fails. Only text and embedded text resources go back to the
session; other content is named, not sent. Be plain about the audit:
the three lows are open at this release.
-->

---

## How it is built

- One bash script and nine `jq` filters in `lib/`: 1,646 lines
- [`usage`](https://usage.jdx.dev), pinned at 7.3.0, reads the `#USAGE`
  block at the top of the script: flags, help, shell completions
- Every `feat:` and `fix:` commit follows a test that is red without it
- 281 bats tests replay recorded SSE and session streams
  (`ASK_CLAUDE_REPLAY`); 5 hurl files check the live API contracts
- GitHub Actions runs the gates on pull requests
- Versions are [EffVer](https://jacobtomlinson.dev/effver/): the number
  that changes says how much effort adopting takes

<!--
Line and test counts are `wc -l` and `grep -c '^@test'` on this
release's tree. The script is three times its size at 0.1.4 and is
still one file; that is the pressure behind what comes next.
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

Coming from 0.2.7, three things can need a change:

- Several `--mcp` servers on the shared `ASK_CLAUDE_MCP_TOKEN`: move to
  one `ASK_CLAUDE_MCP_TOKEN_<HOST>` per server
- An `agent` one-shot prompt starting with the word `tools`: it now
  runs `agent tools`
- A state directory that is a symlink or another account's: point
  `XDG_STATE_HOME` at a directory of your own

<!--
This is why the release is macro: three ways an existing setup can
behave differently. The token and the state directory cases exit 2
before any request; the `tools` case runs another command. For a new user,
`agent` still needs an environment and an agent made in the Console
(`ASK_CLAUDE_ENVIRONMENT`, `ASK_CLAUDE_AGENT`).
-->

---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.2.4

Talk to Claude from a shell with nothing but `curl` and `jq`.

Released 2026-11-05 · EffVer micro · the 0.2 line: Managed Agents

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
ask-claude agent "question"         # a Managed Agents session
ask-claude agent resume <session>   # the API keeps the transcript
```

Messages chats are JSON files under `$XDG_STATE_HOME/ask-claude/`.
A session's transcript lives with the API.

<!--
The split to explain: plain `ask-claude` talks to the Messages API and
keeps history on disk. `ask-claude agent` runs a Managed Agents session,
where the agent, its tools and the transcript are on Anthropic's side.
Same script, same streaming printer.
-->

---

## New since 0.1.4

- **0.2.0** (macro) `ask-claude agent`: one shot, chat, `list` and
  `resume` on Managed Agents sessions; a budget on every session;
  tool confirmations asked at the terminal
- **0.2.1** `-f` uploads a file; `agent files` lists and downloads
- **0.2.2** `--vault`, `--repo` and `--memory` on a session
- **0.2.3** `agent budget`, `agent archive`, `agent trace`
- **0.2.4** `--web` and `--mcp <https url>` on the Messages path

<!--
0.2.0 is macro for two reasons: the word `agent` as a prompt now opens
the command (`ask-claude -- agent` keeps the old meaning), and adopting
it means making an environment and an agent in the Console first.
As of 0.2.3 nothing the spec declares is refused any more.
A web search costs $10 per 1,000; everything else is tokens.
-->

---

## How it is built

- One bash script and eight `jq` filters in `lib/`: 1,197 lines
- [`usage`](https://usage.jdx.dev) reads the `#USAGE` block at the top
  of the script: flags, help, shell completions
- Every `feat:` commit follows a `test:` commit that is red without it
- 154 bats tests replay recorded SSE and session streams
  (`ASK_CLAUDE_REPLAY`); 5 hurl files check the live API contracts
- Versions are [EffVer](https://jacobtomlinson.dev/effver/): the number
  that changes says how much effort adopting takes

<!--
Line and test counts are `wc -l` and `grep -c '^@test'` on this
release's tree. The script more than doubled since 0.1.4; four of the
eight filters are the session printer, folder, history and create
request.
-->

---

## Releases so far

<style scoped>table { font-size: 0.75em; }</style>

| Version | Effort | Date | What |
| --- | --- | --- | --- |
| 0.1.4 | micro | 2026-11-05 | `--base-url`, parity with `claude.sh` |
| 0.2.4 | micro | 2026-11-05 | Managed Agents sessions (0.2.0, macro); `--web`, `--mcp` |

<!--
One row per GitHub release. Versions between rows exist in CHANGELOG.md
but were not released on their own.
-->

---

## Adopting

```sh
mise install                    # jq and usage, pinned; curl from the system
export ANTHROPIC_API_KEY=sk-ant-...
ask-claude "one question"       # with ask-claude on PATH
```

- For `agent`: make an environment and an agent in the Console, then
  set `ASK_CLAUDE_ENVIRONMENT` and `ASK_CLAUDE_AGENT`
- A session's budget is 100 cents unless `--budget` says otherwise
- Exit codes: 0 reply printed, 1 API error, 2 bad input
- Not built in bash: `-p openai`

<!--
Close on cost: the usage line prints the session id, tokens and cost
at list price. The budget is the guard.
`ASK_CLAUDE_CONFIRM` is ask, allow or deny for tool confirmations;
without a terminal a confirmation is denied.
-->

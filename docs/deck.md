---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.2.6

Talk to Claude from a shell with nothing but `curl` and `jq`.

Released 2026-11-06 · EffVer micro · the advisor tool

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

## New since 0.2.5

`--advisor <model>`: the reply can consult another model mid-turn,
through the advisor tool (beta).

- At most 3 advisor calls a turn, billed at the advisor's rates
- The stream shows `(advisor) ` or `(advisor error: <code>) `
- The usage footer adds `advisor=<model> in=… out=…`
- A chat that used an advisor resumes without the flag

Also: the gates now run on every pull request.

<!--
Adopting needs nothing: the flag is new and optional. The CI workflows
arrived between 0.2.5 and this release, ahead of the Rust port, so the
port's pull request would be gated by the same commands as local work.
-->

---

## How it is built

- One bash script and eight `jq` filters in `lib/`: 1,287 lines
- [`usage`](https://usage.jdx.dev), pinned at 7.3.0, reads the `#USAGE`
  block at the top of the script: flags, help, shell completions
- Every `feat:` and `fix:` commit follows a test that is red without it
- 196 bats tests replay recorded SSE and session streams
  (`ASK_CLAUDE_REPLAY`); 5 hurl files check the live API contracts
- GitHub Actions runs the gates on pull requests
- Versions are [EffVer](https://jacobtomlinson.dev/effver/): the number
  that changes says how much effort adopting takes

<!--
Line and test counts are `wc -l` and `grep -c '^@test'` on this
release's tree. The session fixtures are live recordings with the ids
redacted. `just unit-tests` skips the one terminal-bound test, which
`just pty-test` runs alone.
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

---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.2.5

Talk to Claude from a shell with nothing but `curl` and `jq`.

Released 2026-11-06 · EffVer micro · the security audit's fixes

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

## New since 0.2.4

Ten fixes from a security audit (`docs/security-audit/`). Among them:

- `--base-url` checks the parsed host:
  `http://localhost:@example.com` no longer sends the key to example.com
- The `always` answer to a tool confirmation ends with its process
- Every Managed Agents value the script prints goes through `safe`
- Request text reaches `jq` on stdin, never on a command line
- The API's session id is validated before it names a file
- `agent files` writes a fresh temp file, never through a symlink

<!--
Each finding was fixed in its own commit with a regression test. The
other four: the confirmation prompt shows tool input encoded once, the
session fold has explicit defaults so jq 1.8 folds a turn, the recorder
redacts ids as values only, and check-effver validates the parent
VERSION before arithmetic. Nothing to do to adopt: the two behaviour
changes were bugs.
-->

---

## How it is built

- One bash script and eight `jq` filters in `lib/`: 1,247 lines
- [`usage`](https://usage.jdx.dev), pinned at 7.3.0, reads the `#USAGE`
  block at the top of the script: flags, help, shell completions
- Every `feat:` and `fix:` commit follows a test that is red without it
- 170 bats tests replay recorded SSE and session streams
  (`ASK_CLAUDE_REPLAY`); 5 hurl files check the live API contracts
- The session fixtures are live recordings with the ids redacted
- Versions are [EffVer](https://jacobtomlinson.dev/effver/): the number
  that changes says how much effort adopting takes

<!--
Line and test counts are `wc -l` and `grep -c '^@test'` on this
release's tree. The harness says which streams are recorded and which
are derived, and names a missing one. `just unit-tests` skips the one
terminal-bound test, which `just pty-test` runs alone.
-->

---

## Releases so far

<style scoped>table { font-size: 0.75em; }</style>

| Version | Effort | Date | What |
| --- | --- | --- | --- |
| 0.1.4 | micro | 2026-11-05 | `--base-url`, parity with `claude.sh` |
| 0.2.4 | micro | 2026-11-05 | Managed Agents sessions (0.2.0, macro); `--web`, `--mcp` |
| 0.2.5 | micro | 2026-11-06 | Ten fixes from the security audit |

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

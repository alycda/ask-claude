---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.1.4

Talk to Claude from a shell with nothing but `curl` and `jq`.

Released 2026-11-05 · EffVer micro · the first release

<!--
This deck is rewritten at every release, so its history is the project's
history: `git log -p docs/deck.md`. Six slides, about five minutes.
The notes under each slide are for the presenter. Marp shows them in
presenter view; in any other reader they are HTML comments.
-->

---

## What it is

One script, one request shape, streaming always on.

```sh
ask-claude "one question"           # one shot, prints the answer
ask-claude                          # chat loop
ask-claude -f notes.md "summarize"  # file contents + prompt
ask-claude list                     # chats on disk, newest first
ask-claude resume [chat]            # continue a chat
```

A client for the Anthropic Messages API. Chats are JSON files under
`$XDG_STATE_HOME/ask-claude/`.

<!--
Run the first two live if there is a key in the environment. The point to
land: there is no daemon and no config file. The API key is one
environment variable and the state is plain JSON on disk.
-->

---

## New in 0.1.4

The first release. It rolls up 0.1.0 to 0.1.4:

- **0.1.0** one shot: streams the reply, saves both turns
- **0.1.1** `list` and `resume` for the chats on disk
- **0.1.2** the chat loop: `/new`, `/history`, `/quit`
- **0.1.3** `-s` runs skills in the code-execution sandbox
- **0.1.4** `--base-url` (https, or http on localhost); parity with
  `claude.sh`, which the script replaces

<!--
0.1.0 declared every command in the spec and refused the unbuilt ones
with exit 2. Each later version removed refusals. One is left at 0.1.4:
`-p openai`, which is planned for the Rust port, not for bash.
-->

---

## How it is built

- One bash script and four `jq` filters in `lib/`: 523 lines
- [`usage`](https://usage.jdx.dev) reads the `#USAGE` block at the top
  of the script: flags, help, shell completions
- Every `feat:` commit follows a `test:` commit that is red without it
- 83 bats tests replay recorded SSE streams (`ASK_CLAUDE_REPLAY`);
  3 hurl files check the live API contracts
- Versions are [EffVer](https://jacobtomlinson.dev/effver/): the number
  that changes says how much effort adopting takes

<!--
Line and test counts are `wc -l` and `grep -c '^@test'` on this
release's tree. The replay seam is why the unit tests need no key and
no network: the script reads a recorded `.sse` file in place of the
API's stream.
-->

---

## Releases so far

<style scoped>table { font-size: 0.75em; }</style>

| Version | Effort | Date | What |
| --- | --- | --- | --- |
| 0.1.4 | micro | 2026-11-05 | `--base-url`, parity with `claude.sh` |

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

- Needs bash 5.1 for a multi-line paste to be one turn in the chat loop
- Exit codes: 0 reply printed, 1 API error, 2 bad input
- Not built in bash: `-p openai`

<!--
Close on the exit codes: they are the contract for anyone scripting
around it. A refusal from the model is exit 0 with a line on stderr.
-->

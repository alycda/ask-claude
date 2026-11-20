---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.3.2

Talk to Claude from a shell: a bash script, and now a Rust build of it.

Released 2026-11-15 · EffVer meso · the Rust build

<!--
This deck is rewritten at every release, so its history is the project's
history: `git log -p docs/deck.md`. Six slides, about five minutes.
The notes under each slide are for the presenter. Marp shows them in
presenter view; in any other reader they are HTML comments.
-->

---

## What it is

One command line, two implementations, streaming always on.

```sh
ask-claude "one question"           # Messages API: one shot
ask-claude                          # chat loop
ask-claude --web "what changed?"    # the model may search and fetch
ask-claude --mcp <url> --mcp-tool <host>:<tool> "..."   # one MCP tool
ask-claude agent "question"         # a Managed Agents session
ask-claude -p openai "question"     # Rust build only: chat completions
```

Messages chats are JSON files under `$XDG_STATE_HOME/ask-claude/`.
A session's transcript lives with the API.

<!--
Both implementations take the same flags and write the same files, so a
chat started with one resumes with the other. `-p openai` is the
chat-completions shape for any OpenAI-compatible API, by base URL:
`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`. The script still
refuses it.
-->

---

## New since 0.3.1

- A Rust build at parity with the script: the same `#USAGE` interface
  (`just parity` finds no breaking change) and the same bats suite
  green on both
- The script stays and is still the reference
- `ANTHROPIC_BASE_URL` is the API root when `--base-url` is not given;
  one line on stderr names any host that is not `api.anthropic.com`
- A session's approval prompt is asked on the terminal it reads the
  answer from, and keys typed before it appears are discarded
- Bidi formatting characters no longer reach the terminal

<!--
One intended difference between the two: in a session, `a(lways)` in
the Rust build allows the rest of the run's calls of that one tool; the
script's allows every tool. The prompt fix closes a real hole: with
`2> file` the question went to the file while a `y` typed at the
terminal still approved the call.
-->

---

## How it is built

- The script: a bash entry file, seven `lib/*.bash` files and nine
  `jq` filters, 1,920 lines
- The crate in `rust/`: 23 `.rs` files, 11,523 lines, 137 `#[test]`s;
  every dependency pinned to an exact version
- 350 bats tests run against both, over the same recorded streams and
  the same goldens; 7 hurl files check the live API contracts
- `usage diff` compares the script's spec with the one the binary
  prints, so the two command lines cannot drift apart unseen
- Every `feat:` and `fix:` commit follows a test that is red without it
- GitHub Actions runs the gates on pull requests

<!--
Counts are `wc -l`, `grep -c '^@test'` and `grep -c '#\[test\]'` on
this release's tree; the Rust line count includes its tests. The port
was possible in one pull request because the goldens already pinned
what the script prints, byte for byte: the Rust code had a fixed target.
Expect the question about 1,900 lines becoming 11,500. Do not answer it
with a performance claim; the deck has no benchmark.
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
| 0.3.2 | meso | 2026-11-15 | The Rust build at parity; `ANTHROPIC_BASE_URL` |

<!--
One row per GitHub release. Versions between rows exist in CHANGELOG.md
but were not released on their own. The date is the tagged commit's;
CHANGELOG.md's heading can differ from it by a day or more.
-->

---

## Adopting

```sh
cargo install --locked --path rust   # the Rust build, on PATH
# or keep the script: mise install, with lib/ beside ask-claude
export ANTHROPIC_API_KEY=sk-ant-...
ask-claude "one question"
```

- Check whether `ANTHROPIC_BASE_URL` is set in your environment, for
  instance for another tool: ask-claude now sends its requests there,
  and exits 2 if it is not https (or http on localhost)
- Unset it for ask-claude, or pass `--base-url`, to keep the default
- Without it set, nothing changes

<!--
This is the meso part: one environment variable that an adopter may
already have set for something else. `just ask` and the shell
completions use the Rust build from this release on.
-->

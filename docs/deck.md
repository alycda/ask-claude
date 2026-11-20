---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.3.3

Talk to Claude from a shell: a bash script, and a Rust build of it.

Released 2026-11-16 · EffVer meso · `--github`, one `--mcp` server per host

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
ask-claude --github_ro "..."        # GitHub's MCP server, read-only
ask-claude agent "question"         # a Managed Agents session
ask-claude -p openai "question"     # Rust build only: chat completions
```

Messages chats are JSON files under `$XDG_STATE_HOME/ask-claude/`.
A session's transcript lives with the API.

<!--
Both implementations take the same flags and write the same files, so a
chat started with one resumes with the other. `--github_ro` is
`--mcp https://api.githubcopilot.com/mcp/x/repos/readonly` and sends
that long form's request byte for byte. It needs
`ASK_CLAUDE_MCP_TOKEN_API_GITHUBCOPILOT_COM`.
-->

---

## New since 0.3.2

- `--github` and `--github_ro`: shorthands for GitHub's MCP server,
  read and write or the repos toolset read-only
- Two `--mcp` URLs on one host exit 2 before any request: they would
  go out under one name and share one token
- An `--mcp` URL with a query string works
  (`'https://mcp.supabase.com/mcp?project_ref=<ref>&read_only=true'`)
- An `--mcp` value with whitespace exits 2; it used to be split into
  several servers
- `--vault` and `--memory` values are no longer glob-expanded against
  the current directory

<!--
The query-string bug is the story to tell. `usage` hands a repeatable
flag's values over joined with spaces and shell-quotes any value with a
character outside its safe set, and the quotes stayed on. The script
now undoes exactly that quoting, without `eval`. The Rust build never
had it: clap hands over each value as given. It did share the
whitespace split, and now refuses such a value too.
-->

---

## How it is built

- The script: a bash entry file, seven `lib/*.bash` files and nine
  `jq` filters, 2,016 lines
- The crate in `rust/`: 23 `.rs` files, 11,583 lines, 137 `#[test]`s;
  every dependency pinned to an exact version
- 364 bats tests run against both, over the same recorded streams and
  the same goldens; 7 hurl files check the live API contracts
- `usage diff` compares the script's spec with the one the binary
  prints, so the two command lines cannot drift apart unseen
- Every `feat:` and `fix:` commit follows a test that is red without it
- GitHub Actions runs the gates on pull requests

<!--
Counts are `wc -l`, `grep -c '^@test'` and `grep -c '#\[test\]'` on
this release's tree; the Rust line count includes its tests. This
release is the two-implementation setup doing its job: each change
landed in both, and one bug existed in only one of them. It is also the
last release with the script.
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
| 0.3.3 | meso | 2026-11-16 | `--github`, `--github_ro`; one `--mcp` server per host |

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

Check commands and scripts that pass `--mcp`:

- Several URLs in one `--mcp` value now exit 2: give each URL its
  own `--mcp`
- Two `--mcp` URLs on one host now exit 2 before any request: keep one.
  They already failed in 0.3.2, at the API, with exit 1
- Quote a URL with a query string: unquoted, zsh globs the `?` and
  `&` ends the command

<!--
Meso because of the first bullet: a command that worked in 0.3.2 can
now exit 2. The second only changes the exit code and the message of
something that never worked. `ANTHROPIC_BASE_URL` from 0.3.2 still
applies to anyone coming from before it.
-->

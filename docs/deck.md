---
marp: true
title: ask-claude
paginate: true
---

# ask-claude 0.4.0

Talk to Claude from a shell. One binary, one request shape, streaming
always on.

Released 2026-11-19 · EffVer macro · the bash script is removed

<!--
This deck is rewritten at every release, so its history is the project's
history: `git log -p docs/deck.md`. Six slides, about five minutes.
The notes under each slide are for the presenter. Marp shows them in
presenter view; in any other reader they are HTML comments.
-->

---

## What it is

One Rust binary with no runtime dependencies.

```sh
ask-claude "one question"           # Messages API: one shot
ask-claude                          # chat loop
ask-claude --web "what changed?"    # the model may search and fetch
ask-claude --github_ro "..."        # GitHub's MCP server, read-only
ask-claude agent "question"         # a Managed Agents session
ask-claude -p openai "question"     # any OpenAI-compatible API
```

Messages chats are JSON files under `$XDG_STATE_HOME/ask-claude/`.
A session's transcript lives with the API.

<!--
Every command on this slide is the one from the 0.3.3 deck. That is the
point of the release: the command line did not change, the thing behind
it did. The binary makes its own HTTPS requests and starts one other
program, the `--local-mcp` command, through `sh`.
-->

---

## New since 0.3.3

- ask-claude is one Rust binary; the bash script and `lib/` are gone.
  The last version with them: `git show v0.3.3:ask-claude`
- No runtime dependencies. The script needed `curl`, `jq`, `usage`
  and bash 5.1
- It runs where `/bin/bash` is 3.2, which is macOS
- The command line is unchanged: `usage diff` against the script's
  spec at 0.3.3 finds 0 breaking changes, 1 compatible, 11 metadata
- `-p openai`, which the script refused, works
- `ask-claude.usage.kdl` is the tracked record of the spec

<!--
The one compatible change: `agent --budget` accepts a negative number,
which is then refused as not a whole number of cents. The macro label
is worth a sentence. `just effort-floor` says micro, because the floor
measures the command line. The label is macro because every adopter
has to change how they install.
-->

---

## How it is built

- The crate in `rust/`: 23 `.rs` files, 12,212 lines, 162 `#[test]`s;
  every dependency pinned to an exact version
- 266 bats tests run the binary over recorded SSE and session
  streams; 7 hurl files check the live API contracts
- The goldens are frozen: they are what the script printed when it was
  removed, and no second implementation is left to regenerate them
- `just check-spec` fails when `ask-claude.usage.kdl` is stale
- Every `feat:` and `fix:` commit follows a test that is red without it
- GitHub Actions runs the gates on pull requests

<!--
Counts are `wc -l`, `grep -c '^@test'` and `grep -c '#\[test\]'` on
this release's tree; the Rust line count includes its tests. The bats
count fell from 364: the tests of the `jq` filters and of the script's
layout went with the files they tested. A new fixture's golden is now
the binary's own output, read line by line in the commit that adds it.
Next in docs/plan.md: colour in the printer.
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
| 0.4.0 | macro | 2026-11-19 | One Rust binary; the script and `lib/` removed |

<!--
One row per GitHub release. Versions between rows exist in CHANGELOG.md
but were not released on their own. The date is the tagged commit's;
CHANGELOG.md's heading can differ from it by a day or more.
-->

---

## Adopting

```sh
cargo install --locked --path rust   # needs a Rust toolchain
# without a clone:
cargo install --locked --git https://github.com/alycda/ask-claude \
  --tag v0.4.0 ask-claude
export ANTHROPIC_API_KEY=sk-ant-...
ask-claude "one question"
```

- A copy of the script, or a download of the repository's files, no
  longer gives a runnable `ask-claude`: build it once
- `mise install` provides the pinned toolchain
- Flags, environment variables, the state directory and its files are
  the same, so chats and sessions made with the script continue

<!--
The `--git` form is from the GitHub release notes, not the changelog.
The crate is not on crates.io. Close on the last bullet: nothing an
adopter has on disk needs converting.
-->

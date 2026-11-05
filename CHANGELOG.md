# Changelog

Versions follow [EffVer](https://jacobtomlinson.dev/effver/):
`MACRO.MESO.MICRO`, where the number that changes says how much effort it
takes to adopt the change. Macro: a large effort. Meso: some effort. Micro:
no effort. In `0.x` the format is `0.MACRO.MICRO`.

Each heading is `## <version> (<effort>) - <date>`. `just bump <effort>`
adds one, and `just check-effver` checks it against `VERSION`.
`just effort-floor` names the lowest effort a release can claim, from a
`usage diff` of the `#USAGE` spec since the last version.

## 0.1.4 (micro) - 2026-11-05

- The bash `ask-claude` now does everything `claude.sh` did (one shot,
  chat loop, history, skills), with the same request shape and the same
  goldens. Adopting needs no change from 0.1.3.
- `--base-url` sets the API root for `-p anthropic` (https, or http on
  localhost), for a stub or a proxy. `-p openai` stays refused in bash:
  the OpenAI shape, Venice included, is the Rust port's.

## 0.1.3 (micro) - 2026-11-05

- `-s pptx,skill_abc@skver_1` runs the turn with those skills in
  Anthropic's code-execution sandbox; the stream shows `(running code)`
  and `(result rc=N)` while nothing prints. A chat keeps its container
  beside its file (`<chat>.json.container`), so `resume -s ...` reuses
  the files from earlier turns.
- Still refused with exit 2: `-p openai`, `--base-url`.

## 0.1.2 (micro) - 2026-11-05

- `ask-claude` with no prompt opens a chat: each line is a turn, `/new`
  starts another chat, `/history` prints this one, `/quit` exits and
  prints the command to resume it. A multi-line paste is one turn under
  bash 5.1 or newer (macOS `/bin/bash` 3.2 sends on the first newline).
- Still refused with exit 2: `-s`, `-p openai`, `--base-url`.

## 0.1.1 (micro) - 2026-11-05

- `ask-claude list` shows the chats on disk, newest first, and
  `ask-claude resume [chat]` continues one by name or prefix (the newest
  with turns when omitted), printing it first. One-shot replies were
  already saved; now they can be picked up. `list` needs no key.
- Request flags (`-m`, `-e`, `-S`, `--max-tokens`) also apply to `resume`.
- Still refused with exit 2: the chat loop, `-s`, `-p openai`, `--base-url`.

## 0.1.0 (macro) - 2026-11-05

- First version: one-shot. `ask-claude "question"` streams the reply,
  prints a usage line on stderr, and saves both turns. `-f` prepends a
  file; `-m`, `-e`, `-S` and `--max-tokens` shape the request; `-V`
  prints the version.
- Declared but not built yet, and refused with exit 2: `list`, `resume`,
  the chat loop, `-s`, `-p` and `--base-url`.
- To adopt: install `curl`, `jq` and `usage` (or `mise install`), set
  `ANTHROPIC_API_KEY`, put `ask-claude` on `PATH`.

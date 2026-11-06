# Changelog

Versions follow [EffVer](https://jacobtomlinson.dev/effver/):
`MACRO.MESO.MICRO`, where the number that changes says how much effort it
takes to adopt the change. Macro: a large effort. Meso: some effort. Micro:
no effort. In `0.x` the format is `0.MACRO.MICRO`.

Each heading is `## <version> (<effort>) - <date>`. `just bump <effort>`
adds one, and `just check-effver` checks it against `VERSION`.
`just effort-floor` names the lowest effort a release can claim, from a
`usage diff` of the `#USAGE` spec since the last version.

## 0.1.0 (macro) - 2026-11-05

- First version: one-shot. `ask-claude "question"` streams the reply,
  prints a usage line on stderr, and saves both turns. `-f` prepends a
  file; `-m`, `-e`, `-S` and `--max-tokens` shape the request.
- Declared but not built yet, and refused with exit 2: `list`, `resume`,
  the chat loop, `-s`, `-p` and `--base-url`.
- To adopt: install `curl`, `jq` and `usage` (or `mise install`), set
  `ANTHROPIC_API_KEY`, put `ask-claude` on `PATH`.

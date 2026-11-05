# ask-claude

Talk to Claude from a shell with nothing but `curl` and `jq`. One script,
one request shape, streaming always on.

```sh
ask-claude "one question"           one shot, prints the answer
ask-claude                          chat loop
ask-claude -f notes.md "summarize"  file contents + prompt, one shot
ask-claude list                     chats on disk, newest first
ask-claude resume [chat]            continue a chat (name or prefix)
ask-claude --help                   every flag
```

In the chat loop, `/new` starts a fresh chat, `/history` prints the current
one, and `/quit` exits and prints the command to resume it.

## Environment

| Variable | Use |
| --- | --- |
| `ANTHROPIC_API_KEY` | required; there is no config-file fallback |
| `XDG_STATE_HOME` | chats live in `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude/` |
| `ASK_CLAUDE_REPLAY` | testing: read this recorded `.sse` file instead of calling the API |
| `ASK_CLAUDE_REQUEST_LOG` | testing: append each request body to this file |

Nothing else is read.

## Chats on disk

One JSON array of turns per chat, named by its start time
(`20260102T150405.json`). A chat that used skills keeps its container id
beside it in `<name>.json.container`, so the next turn reuses the container.
An empty chat leaves no file.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | reply printed (a refusal also exits 0, with `[stopped: refusal <category>]` on stderr) |
| 1 | the API returned an error; the unanswered turn is not saved |
| 2 | bad input: no key, unreadable `-f` file, unknown chat for `resume` |

## Runtime dependencies

`curl`, `jq`, and [`usage`](https://usage.jdx.dev), which parses the
`#USAGE` block at the top of the script into flags, help, and shell
completions. `mise install` provides `jq` and `usage` at the versions
pinned in `mise.toml`; `curl` comes from the system.

bash 5.1 or newer, so a multi-line paste in the chat loop is one turn.

## Development

Every tool is pinned in `mise.toml`; there are two ways to get them.

**Devcontainer.** Open the folder in VS Code and run *Dev Containers:
Reopen in Container*, or `devcontainer up --workspace-folder .` from a
shell. The mise feature installs mise, and `postCreateCommand` runs
`mise install`, so `jq` and `usage` are on `PATH` once the container is
up. The mise binary comes from the feature's pinned release and does not
update itself; rebuild without the cache to get a newer one.

**On the host.** Install [mise](https://mise.jdx.dev), then:

```sh
mise install                                  # jq and usage, pinned
(umask 077; echo 'ANTHROPIC_API_KEY=sk-ant-...' > .env)   # mode 600; only just test reads it
just --list                                   # the map
```

---

### Resources

- [Bash](https://learnxinyminutes.com/bash/)
- [Curl](https://curl.se/docs/tutorial.html)
- [Hurl](https://hurl.dev/)
- [JQ](https://learnxinyminutes.com/jq/)
- [Usage](https://usage.jdx.dev/)
- [Messages API](https://platform.claude.com/docs/en/build-with-claude/working-with-messages)
- [Justfile](https://github.com/casey/just)

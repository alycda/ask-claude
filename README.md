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

## Agents

`ask-claude agent` runs a prompt, or a chat, as a [Managed Agents](https://platform.claude.com/docs/en/managed-agents/overview)
session: an Anthropic-run sandbox with network, packages, web tools and
MCP servers, as the agent you made allows. The API keeps the transcript.

```
ask-claude agent "one question"          one shot in a new session
ask-claude agent                         chat; /new, /history, /quit
ask-claude agent list                    sessions on the account, newest first
ask-claude agent resume <sesn|prefix>    continue one
```

Once, in the Console (platform.claude.com): an **environment** (start with
limited networking, add `allowed_hosts` for what the job needs) and an
**agent** (model, system prompt, which tools; set bash to *ask* unless you
trust it). `just environments` and `just agents` print their ids. Put them in
`.env` or pass `--environment` and `--agent`:

| Variable | Use |
|---|---|
| `ASK_CLAUDE_AGENT` | the agent id, `--agent` overrides |
| `ASK_CLAUDE_ENVIRONMENT` | the environment id, `--environment` overrides |
| `ASK_CLAUDE_CONFIRM` | `ask` (default), `allow`, or `deny` for tool confirmations |

Every session gets a budget, 100 cents at list price unless `--budget` says
otherwise (`0` for none). Session time bills at $0.08 per active hour on top
of tokens; the usage line shows cost so far. When the agent wants to run a
tool its policy marks *ask*, ask-claude asks you (`y`, `n`, `a` for the rest
of the run) and denies when there is no terminal. A custom tool on the
agent is answered with an error: nothing runs on this machine yet.

A session can be read in the Console under Sessions. Sessions started here
are listed in `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude/sessions/`.

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

`just skills` lists the custom skill ids on the account, for `-s`.
`just completions zsh --install` (or bash, fish) installs tab completion for
the spec, and says what to add to the shell's rc file.
`just test` makes one cheap live call per Anthropic contract,
`just test-agents` one session turn plus the lists (`ASK_CLAUDE_AGENT` and
`ASK_CLAUDE_ENVIRONMENT` in `.env`), and `just test-openai` does the same
for an OpenAI-compatible API (`OPENAI_BASE_URL`, `OPENAI_API_KEY` and
optionally `OPENAI_MODEL` in `.env`; Venice is just a base URL).

---

### Resources

- [Bash](https://learnxinyminutes.com/bash/)
- [Curl](https://curl.se/docs/tutorial.html)
- [Hurl](https://hurl.dev/)
- [JQ](https://learnxinyminutes.com/jq/)
- [Usage](https://usage.jdx.dev/)
- [Messages API](https://platform.claude.com/docs/en/build-with-claude/working-with-messages)
- [Managed Agents](https://platform.claude.com/docs/en/managed-agents/overview)
- [Justfile](https://github.com/casey/just)

# Harness want table: what to take from happy-agent and crush

Input for the Claude Code parity plan (not on this branch yet). The parity
plan answers "what does Claude Code do"; this table answers "what do two
other harnesses do that Claude Code does not, or does worse, and is it worth
building into an ADHD / jj / Ataraxy-native harness".

Sources, read 2026-11-08 from shallow clones of `main`:

- `slopus/happy-agent`: `README.md`, `EVENTS.md`, `docs/configuration.md` headings
- `charmbracelet/crush`: `README.md`, `internal/` package docs, `internal/agent/tools/`, `docs/hooks/`

"Light research" means README-level claims plus a skim of package layout.
Nothing below was run or benchmarked. Rows marked *(mine)* are my proposals,
not features of either tool.

## Lenses

| Lens | Test a feature has to pass |
| --- | --- |
| ADHD | lowers working-memory load, interruptions, or the cost of resuming |
| jj | uses the change graph / op log instead of reinventing history |
| Ataraxy | calm by default: no noise unless action is needed. I don't know Ataraxy Labs' house conventions, so this column only means "quiet by default". Correct it. |

## Adopt

| # | Feature | From | Why, for this harness | Lens | Seam in this repo | Size |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Durable event log with a monotonic cursor; stream deltas are live-only, committed messages and run outcomes are durable | happy `EVENTS.md` | Resume from a cursor instead of re-reading a transcript. The live/durable split keeps the log small | ADHD | `ASK_CLAUDE_EVENTS` log + the MCP server's `since` (U3) already exist; add the split and a cursor id | ~1 day |
| A2 | Steering: a message sent mid-run is folded into the active run (`steering_applied`), not queued behind it | happy | Dump a thought the moment it arrives without waiting for the turn to end and forgetting it | ADHD | needs the agent loop; post-R6 | 2–3 days |
| A3 | Persistent goals: `/goal <objective>`, survives restarts, `pause` / `resume` / `clear` | happy | Externalised working memory: the harness holds "what am I doing" | ADHD | *(mine)* store the goal as the jj change description of the working change, so `jj log` shows it | ~1 day |
| A4 | Rewind to before a chosen user message (`session_rewound`) | happy | Undo a bad turn without starting over | jj | *(mine)* pair chat rewind with `jj op restore` to the op recorded at that turn: one undo covers both chat and files | 1–2 days after J1 |
| A5 | Notify only when the terminal is unfocused, and only on "permission needed" or "turn done"; OSC over SSH, native locally | crush | Lets you leave the terminal without missing the moment it needs you, and stays silent while you watch | ADHD, Ataraxy | new; terminal focus reporting (`CSI ? 1004 h`) | ~half day |
| A6 | Auto-allow only *provably inert* shell commands: parse the command, fall back to a prompt on any redirection, substitution, pipeline, assignment or mutating argument | crush `bash` tool | Fewer prompts with no allowlist to maintain. A conservative parser means a false negative costs only a prompt | ADHD, Ataraxy | Rust shell parser needed; I have not evaluated crates (`brush-parser`, tree-sitter-bash are candidates) | 2–3 days |
| A7 | Per-command secret bundles: shell commands get no secrets by default; each call names the bundles it needs | happy | Same discipline as your per-host `ASK_CLAUDE_MCP_TOKEN_<HOST>`. Don't copy happy's storage: it keeps values as plaintext JSON in SQLite. Resolve at call time from `pass`/age/1Password | — | extends the MCP token rule to shell | ~1 day |
| A8 | Read-before-edit: the edit tool refuses a file the session has not viewed | crush `filetracker` | Stops edits against stale assumptions. Cheap guardrail | — | tool layer | ~2 hours |
| A9 | Session title + one-line recap generated after the first exchange | happy | `ask-claude list` today shows the first-prompt excerpt. A recap answers "what was this chat?" faster | ADHD | `list` output, history file | ~half day |
| A10 | `/review [focus]`: read-only review of the working changes | happy | Parity-adjacent. Makes "review then commit" one step | jj | *(mine)* the target is a revset (default `@`), not "staged + unstaged" | ~half day |

## Parity overlap (Claude Code has these; crush/happy confirm the shape)

These are already in the Claude Code parity plan. Listed only for design notes.

| Feature | Note from the sources |
| --- | --- |
| Hooks | crush has 13 events incl. `TurnStart`, `PreToolUse`; `docs/hooks/FUTURE.md` is a good list of edge cases (subagent opt-in, payload awareness) |
| Skills | crush supports the Agent Skills standard plus `user-invocable` and `disable-model-invocation` frontmatter: model-only, user-only, or both |
| Background jobs | crush: `job_output`, `job_kill` as tools; happy: managed shells |
| Question tool | crush: one pending question at a time, the tool blocks until answered |
| Permission modes | happy: Auto / Workspace write / Read only / Full access, enforced with Bubblewrap on Linux and Seatbelt on macOS, shell network blocked below Full |
| Attribution | crush: trailer style is configurable; default is `Assisted-by:` per the kernel's coding-assistant convention |
| Ignore file | crush: `.crushignore`, gitignore syntax |

## jj-native: the gap neither tool fills

Neither repo mentions jj (one unrelated hit in a crush git-prompt test). All
rows here are *(mine)*.

| # | Feature | What it replaces |
| --- | --- | --- |
| J1 | Each agent turn runs on its own jj change (`jj new` before the first edit; description = the prompt). The turn's id goes into the event log | crush's per-session file version service; Claude Code's checkpoints |
| J2 | Record the jj op id at each turn boundary in the event log | gives A4 (rewind) a target, no separate snapshot store |
| J3 | `jj absorb` / `jj squash` as harness verbs after a turn | manual cleanup of agent-made commits |
| J4 | Subagents in separate `jj workspace`s, sharing one repo | git worktrees in Claude Code |
| J5 | Attribution as a trailer on the jj change, written by the harness, not the model | model-written trailers that drift |

## Considered, not adopted

| Feature | From | Why not (now) |
| --- | --- | --- |
| Bash as the config language (`crushrc`, built-in interpreter) | crush | Config becomes trusted code (crush's own README warns about it). Declarative config fits Nix better; the usage spec already defines the CLI surface |
| Foreign harnesses over ACP (run Claude Code / Codex as the loop) | happy | The point is learning by building the loop yourself. Revisit after R7 |
| Daemon + typed client + mobile sync | happy | Large surface. The MCP server in `mcp/` is the external seam for now |
| Shared workspaces across clients (`IsBusy`, `AttachedClients`) | crush | Needs a daemon; same reason |
| LSP tools (definition, rename, symbols, diagnostics) | crush | High value for Rust (rust-analyzer), high cost. Revisit after parity |
| Auto mode: a one-call model review of risky actions, fail-closed, temporary escalation labelled in history | happy | Good design, but depends on A6 and the permission modes existing first |

## Practice worth copying

happy's README has an explicit scope list: it states it will not add Plan
mode, Vim mode, a notebook editor, a durable allow/deny history, or an IDE
integration. The parity plan needs the same section, so "parity" has an edge.

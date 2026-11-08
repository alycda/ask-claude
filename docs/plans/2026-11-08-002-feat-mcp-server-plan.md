---
title: "feat: an MCP server for ask-claude's local state, stdio first"
type: feat
date: 2026-11-08
---

# feat: an MCP server for ask-claude's local state, stdio first

## Summary

`ask-claude-mcp`, a Rust binary in `mcp/` built on the official SDK
([rmcp](https://github.com/modelcontextprotocol/rust-sdk) 3.9), serves what
the script keeps on disk to any MCP client: the README's five event-log
questions as read-only tools, and the chats and the event log as resources.
0.4.0 runs over stdio only and calls no model. The Managed Agents tools,
HTTP, a recording mode for `--mcp` and `second_opinion` come after it, as
TODO commits in the same stack.

It comes before the Rust port of the script (phase 4) and does not wait for
it: the reports read files, and the later agent tools run the script, the
one implementation, until the port's R6/R7 can replace it.

---

## Decisions (settled 2026-11-08)

| Decision | Choice | Consequence |
| --- | --- | --- |
| Backend | the agent tools run the bash script; local state is read directly | one implementation of the API calls; the replay seam tests both |
| First transport | stdio | no auth to design yet; the client that starts the server is the only one that reaches it |
| First scope | local state: 5 reports, chats and log as resources | no key, no network, no spend |
| `agent_start` | returns once the session exists | needs `agent --detach` and `agent history` in the script first |
| Crate | `mcp/`, its own crate, not in `rust/` | no conflict with the port's line; a workspace can join them after R8 |
| Oracle | the README's jq queries, verbatim in `tests/fixtures/mcp/reports/*.jq` | the goldens are jq's output; `mcp-reports.bats` keeps README and fixtures equal |
| Test seam | a black-box JSON-RPC client over stdio (`mcp/tests/common`) | the tests pin the wire, not rmcp's types, which move fast |
| Writes | none, ever | no state dir or log is created; no mode is changed |

---

## What the server serves (0.4.0)

| Tool | Answers | README query |
| --- | --- | --- |
| `tokens_by_chat` | tokens per chat; sessions take their last total | Tokens per chat |
| `cache_hit_by_model` | share of input read from the cache, per model | Cache hit rate by model |
| `seconds_by_model` | requests, mean and max seconds, per model | Seconds by model |
| `refusals_by_category` | refusals per stop category | Refusals by category |
| `advisor_spend` | calls and tokens per advisor model | Advisor spend by advisor model |

Each takes one optional argument, `since` (`YYYY-MM-DD` or
`YYYY-MM-DDTHH:MM:SSZ`), which keeps the events whose `ts` compares at or
after it ("advisor spend this week"). Each answers one text block holding
the report as JSON. All five are `readOnlyHint: true, openWorldHint:
false`.

| Resource | Is |
| --- | --- |
| `ask-claude://chats/<name>` | a chat with turns, `application/json`, the file as is; newest first |
| `ask-claude://events` | `events.jsonl`, `application/jsonl`, the file as is |

The server reads `${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude` on every
call and refuses it the way the script does: a symlink, another account's
directory, one any account can write.

---

## Units

Each unit is a red `test:` commit and an empty `TODO:` child that says what
turns it green; the feat replaces the TODO, and the test does not change
(docs/plan.md's rule). The library's signatures in `mcp/src/lib.rs` are a
starting shape: change one in the commit that needs it, and say why.

| # | Test commit | Files | Green when | Status |
| --- | --- | --- | --- | --- |
| — | chore: rust 1.99.1 in the toolchain | `mise.toml`, `mise.lock` | the same change as the port's line, duplicated | [x] |
| — | chore: mcp/ crate scaffold | `mcp/`, `justfile` (mcp-*), `.github/workflows/mcp.yml` | `just mcp-build` | [x] |
| — | test: the MCP harness | `mcp/tests/common`, `mcp/tests/harness.rs` | green from the start, on the script's fake server | [x] |
| U1 | test: introduces itself, lists the five reports | `mcp/tests/protocol.rs`, `tests/unit/mcp.bats` | handshake, tools/list, exit 0 at EOF; the script's client lists the tools | [t] |
| U2 | test: five reports, jq as the oracle | `mcp/tests/reports.rs`, `tests/fixtures/mcp/**`, `tests/unit/mcp-reports.bats` | library and server equal the goldens; an agent's custom tool call reaches a report | [t] |
| U3 | test: since | `mcp/tests/since.rs` | the `.since.json` goldens; a bad since is a tool error | [t] |
| U4 | test: a missing, empty or torn log | `mcp/tests/log.rs` | `[]` with no log; torn and foreign lines skipped; unreadable is a tool error | [t] |
| U5 | test: the state-dir gate, read side | `mcp/tests/gate.rs` | symlink and world-writable refused, on every call, no chmod | [t] |
| U6 | test: chats and the log as resources | `mcp/tests/resources.rs` | list, read byte for byte, -32002 for anything else | [t] |
| V | TODO(verifier): the 0.4.0 gate | | the commands in its message, recorded | [ ] |

Run them with `just mcp-test` (Rust), `just mcp-bats` (the script as the
client) and `just unit-tests` (the jq oracle, no server). CI runs all three
in `.github/workflows/mcp.yml`, soft until version(0.4.0) turns the soft
steps hard.

## After 0.4.0

TODO commits only; each becomes test/feat pairs when it is next.

| TODO | Needs |
| --- | --- |
| TODO(decision): the HTTP transport, and who may call it | a decision on auth (bearer for `--mcp`, OAuth for claude.ai) and on what HTTP may serve |
| TODO: `agent --detach` and `agent history` in the script | bats test/feat pairs, spec entries |
| TODO: the agent tools, over the script | the TODO above; HTTP needs the auth decision |
| TODO: a recording mode, the fixture for `--mcp` | HTTP |
| TODO: `second_opinion`, after the port's R7 | the `Provider` trait |
| TODO: version(0.5.0) | the above |

## Out of scope

Prompts, sampling, elicitation, subscriptions and `listChanged`: nothing
here changes while a client watches that a re-list does not cover. Writing
anything to the state directory. Serving the chats over HTTP by default.

## Risks

- rmcp 3.x moves fast (3.9 knows protocol 2026-07-28, which drops the
  `initialize` handshake). The crate pins `=3.9.0`, and the tests speak
  2025-06-18, the version the script's `--local-mcp` client sends, so a
  bump that breaks older clients fails the suite.
- The state-dir gate is two lines apart: 0.3.0 refuses any group-writable
  STATE, and the audit-run-2 line accepts the user's own primary group and
  adds an ancestors check. The tests pin only what both refuse; U5's TODO
  says to follow whichever the stack merges with.
- The refactor line that moves the script into `lib/*.bash` touches
  `ask-claude`, `lib/` and one README line, none of which this stack
  changes; `mcp-reports.bats` reads the README's queries, so a change
  there shows up as a red oracle test, not a merge conflict. The MCP
  workflow watches `lib/**` for the same reason.

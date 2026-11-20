---
date: 2026-11-07
topic: request-event-log
---

# Request event log

## Summary

ask-claude appends one private JSON line per API request to a local event log. The line holds the token, timing and outcome facts the footer already prints, and never any prompt or reply text. `jq` over that file answers the questions about cost, cache hits, latency, refusals and advisor spend. The Rust port writes the same lines.

---

## Problem Frame

Every request prints a footer with model, tokens, cache reads and writes, advisor tokens, seconds and stop reason, and then that information is gone. Answering "what did this chat cost", "is caching working" or "how often am I refused" means scrolling terminal history. A run lasts seconds, so there is no long-lived process for a metrics system to scrape. The useful unit is the single request, and the script already computes almost every field of it.

---

## Key Decisions

- **A local file, not OTel.** The goal is cost and latency, not cross-request traces. A JSON Lines file needs no infrastructure and is readable by `jq`. OTel export, a tracing backend and Datadog are out (see Scope Boundaries).
- **Tokens, not dollars.** The log records what the API reports. Neither the script nor a checked-in query carries a price table, so nothing goes stale when prices change. Session events keep the API's own `list_cost`, because the API returns it. Dollar figures therefore exist for sessions only.
- **One line per request, not per turn.** A `pause_turn` continuation and a session's custom-tool round are separate requests. Each gets its own line, carrying its round number, so a turn's total is a sum.
- **On by default, one switch off.** The log is strictly less sensitive than the history files the script already keeps, so it needs no opt-in. One environment variable turns it off.
- **Host, not full base URL.** The provider is recorded as the API host. Any path, query or credentials in a user-supplied base URL never reach the log.

---

## Requirements

**What is logged**

- R1. Every completed Messages request appends one line: timestamp, chat id, model, input, cache-read, cache-write and output tokens, stop reason, stop-details category when present, seconds, API host, and the `pause_turn` round.
- R2. When the reply used the advisor, the same line carries the advisor model, its call count, and its input and output tokens.
- R3. Every Managed Agents session turn appends one line with the session id, the usage the footer prints including `list_cost`, stop reason, seconds and custom-tool round.
- R4. A failed request (no response, an unparsable reply, an API error) appends one line with an error category and no error message text.
- R5. Each line carries a schema version and the ask-claude version.

**Privacy and storage**

- R6. No line ever contains prompt text, reply text, file contents, tool inputs, error message bodies or the API key.
- R7. The log lives in the state directory beside the history, created private the way the history is.
- R8. The log is append-only. The script never rotates, truncates or rewrites it.
- R9. Setting the off switch writes nothing and creates no file.
- R10. Failing to write the log never fails the request, and never changes stdout, the footer or the exit code.

**Querying**

- R11. The README documents the log's location, its fields, the off switch, and a few `jq` queries: tokens per chat, cache hit rate, latency by model, refusals by category, and advisor tokens.

**Rust port**

- R12. The Rust binary writes the same lines to the same file, and the same tests check both binaries.

---

## Acceptance Examples

- AE1. **Covers R1, R6.** Given a replayed `text.sse` one-shot, when it finishes, the log has one line with model, tokens, `end_turn` and round 0, and the prompt string appears nowhere in the file.
- AE2. **Covers R1.** Given a turn that returns `pause_turn` once and then `end_turn`, the log has two lines with rounds 0 and 1.
- AE3. **Covers R2.** Given a reply whose usage holds advisor iterations, the line carries the advisor model, its call count and its token sums. Without advisor iterations, those fields are absent or null.
- AE4. **Covers R4, R6.** Given a replayed `error.sse`, the run exits 1 as today and the log has one line with an error category and no message text.
- AE5. **Covers R9.** Given the off switch is set, a replayed one-shot leaves no log file.
- AE6. **Covers R10.** Given a log path that cannot be written, a replayed one-shot prints the same stdout and footer and exits 0.

---

## Scope Boundaries

**Deferred for later**

- `tracing` spans and OTLP export in the Rust port. Revisit if tool-loop or Managed Agents tracing becomes a real need. Export would be off unless an OTLP endpoint is set.
- A Grafana, Tempo and Loki backend, as the `otel-lgtm` image or as Nix-declared services. This only matters once OTLP export exists.
- A `stats` subcommand. The README queries come first, and a subcommand is worth adding only if the same query keeps being retyped.

**Out**

- Datadog and any other SaaS backend. They are billed per host and per metric and are more than a single-user CLI needs.
- Prices and dollar estimates for the Messages path.
- DuckDB as a dependency. `jq` covers the documented queries, and DuckDB can read the file without the project shipping it.

---

## Dependencies / Assumptions

- This builds on v0.2.6, which adds the advisor fields to the Messages footer.
- The existing seams cover the tests: `XDG_STATE_HOME` for the state directory and `ASK_CLAUDE_REPLAY` for offline runs (`docs/plan.md`, Decisions).
- Assumption: the chat id is the history file's name, the same id `list` and `resume` show.

---

## Outstanding Questions

**Deferred to Planning**

- The off switch's name, and whether the log path can also be overridden for tests.
- The error-category vocabulary, for example `no_response`, `parse` and `api` with the HTTP status.
- Whether the log line is built by a new `lib/*.jq` program that gets goldens like the others, which gives the Rust port one shape to match.

---

## Sources

- `ask-claude`: the Messages footer and the `pause_turn` loop (the `call` function), the session footer, `umask 077` and `STATE`.
- `lib/session-fold.jq`: session usage fields, including `list_cost`.
- [Feat(0.2.6): Advisor](https://github.com/alycda/ask-claude/pull/4), `ask-claude`: the advisor sums from `usage.iterations`.
- `docs/plan.md`: test seams, the jq file contract, Phase 4 (Rust).

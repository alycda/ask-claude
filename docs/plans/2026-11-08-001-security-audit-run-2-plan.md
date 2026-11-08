---
title: "security audit, run 2: a scoped re-run of the Cloudflare security-audit skill"
type: security
date: 2026-11-08
origin: docs/security-audit/REPORT.md
status: done, 2026-11-07
result: docs/security-audit/
---

# security audit, run 2: a scoped re-run of the Cloudflare security-audit skill

## Summary

A second full-audit-mode run of the [Cloudflare security-audit skill](https://github.com/cloudflare/security-audit-skill), scoped to the source that changed since run 1 (`88afb08`) and to the work run 1 left open. The plan below is as written before the run. The run happened at `9c21790e`, and its report replaced `docs/security-audit/` in place, so run 1 survives in git history.

## Outcome

The run reached the skill's `complete` state, with both validators passing (8 findings, 28 coverage units). `docs/security-audit/REPORT.md` is the record. It departed from this plan in three ways:

- **Budget:** 25 was raised to 32 after wave 1, because 8 candidates needed about 18 more agents. 30 were spent.
- **Sandbox:** the run was on macOS, so `tools/security-audit/sandbox` (Linux-only) was not used. Target code ran in a Docker sandbox instead: no network, read-only except a per-agent scratch directory, an allowlisted environment, and the same limits.
- **Coverage:** the post-wave critic accepted one unit that the budget could not fund, the `effver.just` bump recipe and the `bump-effver` VERSION sink. It is deferred, so the run makes no clean-coverage claim.

Results:

- **3 confirmed, all low:** a jq 1.7 `tonumber` error that bypasses `safe`; the custom-tool round cap; per-host MCP token name collisions.
- **5 needs_validation**, each with a blocker the source cannot settle.
- **Run 1:** its 8 confirmed records and its shared-token lead are fixed in current source.

The machine-readable plan that seeded the run (`*.run-metadata.json`) was removed after the run. The run's real metadata names local paths, and the report states its facts.

---

## Why a second run

- **The posture claim no longer holds.** Run 1's report says ask-claude "never executes model or tool output locally". Since `agent --local-mcp`, model-chosen custom tool calls run on the user's machine, behind a tool confirmation. That code was written and tested by its author only.
- **Run 1 left work open.** Its two coverage critics accepted 4 units that it deferred for budget and never reviewed.
- **Run 1's records point at changed source.** Its 8 confirmed findings were fixed in 0.2.5, and its 2 needs-validation leads were fixed on `claude/eloquent-meitner-cjbls7`: the state-dir gate in `745a64c`, one MCP token per server in `c61df4e`. Under the skill's prior-run rules, each changed-source record becomes a current revalidation unit, not a carried result.
- **New code since run 1:** the event log, `--advisor`, the `agent files` listing, the CI workflows, and changes to `tools/effver/`.

---

## Scope

Planned coverage units. The skill may merge or split them in reconnaissance.

New or changed surface:

1. `--local-mcp`: server start (`sh -c`), the FIFO transport, the process group, lifecycle and cleanup.
2. `--local-mcp`: model-chosen custom tool calls reaching local execution, the confirmation path, `ASK_CLAUDE_CONFIRM`, and the always grant.
3. `--local-mcp`: tool results and server-controlled strings flowing to the API and to the terminal (the `AI-AND-LLM.md` companion).
4. `agent tools`: server-supplied tool definitions.
5. `lib/event.jq` and `events.jsonl`: lines built from endpoint-controlled strings.
6. `--advisor`: request building and stream rendering.
7. `agent files`: the new listing, and the run-1 fix for downloads.
8. `.github/workflows/`: triggers, permissions and action pinning (the `SUPPLY-CHAIN-AND-RELEASE.md` companion).

Revalidation of fixed records:

9. The state-dir gate: run 1's state-dir record and the resolved lead `state-dir/owner-and-parent-unchecked`.
10. `--mcp` tokens: the resolved lead `mcp_servers:shared-authorization-token`, now per-host variables and a refusal.
11. `tools/effver/check-effver` and `effver.just`: run 1's one medium finding and its fix.
12. The other run-1 confirmed records whose source changed, 8 in all with item 11.

Run-1 deferred units:

13. Messages `--web --mcp -s -f` tool composition.
14. `agent --repo --vault --memory` session resources.
15. `session_stream_read` stream framing.
16. `check-effver` problem-report printing.

Out of scope: `tests/` except as evidence, `docs/`, and the units run 1 covered whose source has not changed. The report must present itself as a scoped, partial run.

---

## Profile and budget

`standard`, budget 25 agent invocations. The skill's gate reserves 4 reconnaissance agents, a post-wave critic and a final-clean critic, and verifiers (about 2 per candidate) before any hunter runs. Units the budget cannot reach are recorded as `deferred` in the ledger, not dropped.

The alternative the owner was offered: `quick`, about 20 agents, with one hunter wave, one critic and one verifier per candidate for both checks.

---

## Execution boundary

- **Sandbox:** target code runs only under `tools/security-audit/sandbox`, which uses user, network, mount and PID namespaces:
  - no external network
  - every mount read-only except the agent's scratch directory
  - an empty environment from an allowlist, with a dummy API key
  - 60 s wall clock, 30 s CPU, 512 MiB, 64 processes, 16 MiB per file
- **Checked:** in a cloud container, network access and writes to the target, `/`, `/tmp`, `/var/tmp`, `/run` and `/dev/shm` all failed inside it, and nothing reached the host.
- **Limits:** Linux only. Docker is not used. If a control cannot be enforced on the machine that runs the audit, the skill's rule applies: target code does not run, and the decisive check becomes a needs-validation blocker.
- **No live traffic:** no deployed endpoint, third-party MCP server or paid API is contacted, and every secret is a dummy.
- **Skill:** `cloudflare/security-audit-skill` at commit `c1c8a8c`, read from a checkout outside the target. Run 1's `coverage-ledger.json` and `findings.json` pass that commit's validators (20 units, 11 findings).

---

## Output

The skill writes to `~/security-audit-skill/ask-claude/run-2/`, outside the target. When both validators pass, `REPORT.md`, `FINDINGS-DETAIL.md`, `NEEDS-VALIDATION.md`, `findings.json` and `coverage-ledger.json` replace the files in `docs/security-audit/`.

---

## Before starting

- Resolve the source ref at start (`git rev-parse HEAD`). The plan was written at `8cbbcd5`.
- Re-check the sandbox on the machine that runs the audit, with the probes above.
- Confirm the profile and budget. A 25-agent run is above a default session's guidance of under 10 agents, so it needs the owner's explicit go-ahead.

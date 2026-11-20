#!/usr/bin/env bash
# record-session.sh: record one Managed Agents session as an SSE fixture.
#
#   tests/record-session.sh <name> "<prompt>" [<fixture dir>]
#
# Needs ANTHROPIC_API_KEY, ASK_CLAUDE_AGENT and ASK_CLAUDE_ENVIRONMENT (the
# agent and environment made in the Console; the fixtures came from an agent
# on claude-haiku-5-5 with web tools off and bash set to always_ask, in an
# environment with limited networking and no hosts). Creates the session with
# a 10-cent budget, streams until the first session.status_idle, and writes
# <dir>/<name>.sse with every id replaced by a stable FIXTURE id. A session
# that stops with requires_action is left for you to answer by hand; stream it
# again with `ASK_CLAUDE_SESSION=<sesn> tests/record-session.sh <name>-2`.
# The ids are the only thing redacted: keep prompts free of anything private.
set -euo pipefail
name="${1:?name}"
prompt="${2:-}"
dir="${3:-$(dirname "$0")/fixtures/sessions}"
: "${ANTHROPIC_API_KEY:?}" "${ASK_CLAUDE_AGENT:?}" "${ASK_CLAUDE_ENVIRONMENT:?}"
api="${ASK_CLAUDE_BASE_URL:-https://api.anthropic.com}/v1"
headers=$(mktemp)
trap 'rm -f "$headers"' EXIT
chmod 600 "$headers"
printf 'content-type: application/json\nx-api-key: %s\nanthropic-version: 2026-10-21\nanthropic-beta: managed-agents-2026-04-01\n' "$ANTHROPIC_API_KEY" >"$headers"

if [ -n "${ASK_CLAUDE_SESSION:-}" ]; then
  sesn="$ASK_CLAUDE_SESSION"
else
  [ -n "$prompt" ] || { echo "record-session: a prompt is needed for a new session" >&2; exit 2; }
  sesn=$(jq -n --arg a "$ASK_CLAUDE_AGENT" --arg e "$ASK_CLAUDE_ENVIRONMENT" --arg t "fixture $name" --arg p "$prompt" '
    {agent:{type:"agent",id:$a}, environment_id:$e, title:$t,
     budget:{type:"limit", max_list_cost:{amount:"10", currency:"USD"}},
     initial_events:[{type:"user.message", content:[{type:"text", text:$p}]}]}' |
    curl -sS "$api/sessions" -H @"$headers" --data-binary @- | jq -r '.id // error(.error.message)')
  echo "session $sesn" >&2
fi
# sed quits at the first idle, which closes the pipe and ends curl (exit 23)
curl -sS -N --max-time 120 "$api/sessions/$sesn/events/stream" -H @"$headers" |
  sed -u '/"type":"session.status_idle"/q' |
  awk '
    { line = $0; out = ""
      while (match(line, /(sevt|sesn|agent|env|sthr|vlt)_[A-Za-z0-9]+/)) {
        id = substr(line, RSTART, RLENGTH)
        if (!(id in map)) { split(id, p, "_"); n[p[1]]++; map[id] = p[1] "_FIXTURE" sprintf("%02d", n[p[1]]) }
        out = out substr(line, 1, RSTART - 1) map[id]
        line = substr(line, RSTART + RLENGTH)
      }
      print out line }' |
  sed 's/"agent_name":"[^"]*"/"agent_name":"fixture"/' >"$dir/$name.sse" || true
echo "wrote $dir/$name.sse ($(grep -c '^data:' "$dir/$name.sse") events); stop: $(grep -o '"stop_reason":{[^}]*}' "$dir/$name.sse" | tail -1)" >&2

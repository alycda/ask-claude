# lib/session.jq: print a Managed Agents session stream while it arrives.
#
# Invocation (see tests/unit/session.bats):
#     sed -n 's/^data: //p' session.sse | jq --unbuffered -rj -f lib/session.jq
#
# Like lib/stream.jq: one event per line, one string out per event, "" for
# the events a terminal does not need (spans, thread status, usage). The
# agent's text comes as whole agent.message events, not deltas; a tool call
# prints its name while nothing else is printing; an idle ends the line.
# Everything goes through `safe` (same definition as lib/stream.jq).

def safe:
    if type == "string" then
        explode | map(select(. == 9 or . == 10 or (. >= 32 and . != 127 and (. < 128 or . > 159)))) | implode
    else . end;

(if .type == "agent.message" then
    [.content[]? | select(.type == "text") | .text] | join("")
elif .type == "agent.thinking" then
    "(thinking) "
elif .type == "agent.tool_use" or .type == "agent.custom_tool_use" then
    "(\(.name // "tool")) "
elif .type == "agent.mcp_tool_use" then
    "(mcp \(.name // "?")) "
elif .type == "session.status_idle" or .type == "session.status_terminated" then
    "\n"
else
    ""
end) | safe

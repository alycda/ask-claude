# lib/session-history.jq: a session's messages as "you:" / "agent:" text.
#
# Invocation (see tests/unit/agent-chat.bats):
#     curl .../v1/sessions/<id>/events?types[]=user.message&types[]=agent.message \
#       | jq -r -f lib/session-history.jq
#
# The input is one events page ({data: [...]}); other event types in it are
# skipped. Same `safe` as lib/stream.jq: the text is the model's.

def safe:
    if type == "string" then
        explode | map(select(. == 9 or . == 10 or (. >= 32 and . != 127 and (. < 128 or . > 159)))) | implode
    else . end;

.data[]?
| select(.type == "user.message" or .type == "agent.message")
| (if .type == "user.message" then "you" else "agent" end) as $who
| ([.content[]? | select(.type == "text") | .text] | join("\n")) as $t
| "\($who): \($t)\n" | safe

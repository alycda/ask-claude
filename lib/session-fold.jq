# lib/session-fold.jq: fold one stream of a Managed Agents session into what
# the shell decides on after the stream stops at an idle.
#
# Invocation (see tests/unit/session.bats):
#     sed -n 's/^data: //p' session.sse | jq -s -f lib/session-fold.jq
#
# Output, keys in this order (tests/fixtures/sessions/*.fold.json):
#   stop_reason  end_turn | requires_action | budget_reached | terminated | null
#   event_ids    the ids the session is waiting on (requires_action)
#   asks         the agent.tool_use events among them: {id, name, input}
#   custom       the agent.custom_tool_use events among them: {id, name, input}
#   usage        from the last session.usage: tokens, cache_read, cache_write
#                (5m + 1h), list_cost (cents, a string), active_seconds
#   error        the last session.error's error, or null
#
# Every last(...) has an explicit // default: since jq 1.8, last(empty) is
# empty, not null, and one empty value would empty the whole object.

(last(.[] | select(.type == "session.status_idle" or .type == "session.status_terminated")) // null) as $idle
| ($idle.stop_reason.event_ids // []) as $ids
| def waiting: .id as $i | $ids | index($i) != null;
  { stop_reason: (if $idle.type == "session.status_terminated" then "terminated" else ($idle.stop_reason.type // null) end),
    event_ids: $ids,
    asks: [ .[] | select(.type == "agent.tool_use" and waiting) | {id, name, input} ],
    custom: [ .[] | select(.type == "agent.custom_tool_use" and waiting) | {id, name, input} ],
    usage: (((last(.[] | select(.type == "session.usage")) | .usage) // {}) as $u
            | { input_tokens: ($u.input_tokens // 0),
                output_tokens: ($u.output_tokens // 0),
                cache_read: ($u.cache_read_input_tokens // 0),
                cache_write: (($u.cache_creation.ephemeral_5m_input_tokens // 0) + ($u.cache_creation.ephemeral_1h_input_tokens // 0)),
                list_cost: ($u.list_cost.amount // "0"),
                active_seconds: ($u.active_seconds // 0) }),
    error: ((last(.[] | select(.type == "session.error")) | .error) // null) }

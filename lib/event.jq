# lib/event.jq: one line of the event log, $STATE/events.jsonl.
#
# Invocation (see tests/unit/event.bats):
#     jq -c --arg ts ISO8601 --arg version X.Y.Z --arg kind messages|session \
#           --arg chat NAME --arg host HOST --argjson round N \
#           --argjson seconds N --arg error CATEGORY -f lib/event.jq
#
# Input: the reassembled message (kind messages), the session fold (kind
# session, lib/session-fold.jq), or null when the request failed before
# either existed.
#   $error    the failure's category ("" means none): the API's error.type,
#             or no_response | parse | unreadable | interrupted | unanswerable
#   $round    the pause_turn round (messages) or custom-tool rounds (session)
#
# Never written: prompt or reply text, tool inputs, error messages. Every
# string that came from the endpoint (or from the shell) is kept only when it
# looks like an identifier, so a proxy behind --base-url cannot put request
# text into the log. Session usage is the session's so far (usage_scope).
#
# The tests compare with tests/fixtures/events/*.jsonl byte for byte, so the
# key order below is part of the format: the Rust port writes the same.

def ident: if . == null then null
  elif type == "string" and test("^[A-Za-z0-9._:-]{1,64}$") then .
  else "unknown" end;
def hostname: if type == "string" and test("^[A-Za-z0-9.:\\[\\]-]{1,255}$") then . else "unknown" end;

. as $in
| ($kind == "session") as $s
| ($in.usage // null) as $u
| ([$u.iterations[]? | select(.type == "advisor_message")]) as $a
| { v: 1,
    ask_claude: ($version | ident),
    ts: ($ts | ident),
    kind: (if $s then "session" else "messages" end),
    chat: ($chat | ident),
    host: ($host | hostname),
    model: (if $s then null else $in.model // null | ident end),
    round: $round,
    seconds: $seconds,
    stop_reason: ($in.stop_reason // null | ident),
    stop_category: ($in.stop_details.category // null | ident),
    usage_scope: (if $s then "session" else "request" end),
    input_tokens: $u.input_tokens,
    output_tokens: $u.output_tokens,
    cache_read: (if $u == null then null elif $s then $u.cache_read else $u.cache_read_input_tokens // 0 end),
    cache_write: (if $u == null then null elif $s then $u.cache_write else $u.cache_creation_input_tokens // 0 end),
    list_cost: (if $s then $u.list_cost // null | ident else null end),
    advisor: (if ($a | length) > 0 then
                { model: ($a[0].model | ident), calls: ($a | length),
                  input_tokens: ($a | map(.input_tokens // 0) | add),
                  output_tokens: ($a | map(.output_tokens // 0) | add) }
              else null end),
    error: (if $error == "" then null else $error | ident end) }

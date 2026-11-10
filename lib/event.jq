# lib/event.jq: one line of the event log, $STATE/events.jsonl.
#
# Invocation (see tests/unit/event.bats):
#     jq -c --arg ts ISO8601 --arg version X.Y.Z --arg kind messages|session \
#           --arg chat NAME --arg host HOST --argjson round N \
#           --argjson seconds N --arg error CATEGORY [--argjson tools TOOLS] \
#           -f lib/event.jq
#
# Input: the reassembled message (kind messages), the session fold (kind
# session, lib/session-fold.jq), or null when the request failed before
# either existed.
#   $error    the failure's category ("" means none): the API's error.type,
#             or no_response | parse | unreadable | interrupted | unanswerable
#   $round    the pause_turn round (messages) or custom-tool rounds (session)
#   $tools    the request's tools array (messages), or null; optional, read
#             through $ARGS.named so the callers of 0.3.0 still work
#
# Never written: prompt or reply text, tool inputs, error messages. Every
# string that came from the endpoint (or from the shell) is kept only when it
# looks like an identifier, so a proxy behind --base-url cannot put request
# text into the log. Session usage is the session's so far (usage_scope).
#
# The tests compare with tests/fixtures/events/*.jsonl byte for byte, so the
# key order below is part of the format: the Rust port writes the same.
# \A and \z, not ^ and $: in Oniguruma $ also matches before a final newline
def ident: if . == null then null
  elif type == "string" and test("\\A[A-Za-z0-9._:-]{1,64}\\z") then .
  else "unknown" end;
def hostname: if type == "string" and test("\\A[A-Za-z0-9.:\\[\\]-]{1,255}\\z") then . else "unknown" end;
# a tool's entry in .tools: built from our own request, so not endpoint text,
# but bounded the same way (an MCP tool name runs to 128 characters)
def toolname: if type == "string" and test("\\A[A-Za-z0-9._:-]{1,255}\\z") then . else "unknown" end;
# token counts: a number, or null; a string here could be anything
def num: if type == "number" then . else null end;
# cents, as the API gives them (a string), or a number made into one
def cents: if . == null then null elif type == "number" then tostring
  elif type == "string" and test("\\A[0-9]+(\\.[0-9]+)?\\z") then .
  else "unknown" end;

(if type == "object" then . else null end) as $in
| ($kind == "session") as $s
| ($in.usage // null | if type == "object" then . else null end) as $u
| ([$u.iterations[]? | select(type == "object" and .type == "advisor_message")]) as $a
| { v: 1,
    ask_claude: ($version | ident),
    ts: ($ts | ident),
    kind: (if $s then "session" else "messages" end),
    chat: (if $chat == "" then null else $chat | ident end),
    host: ($host | hostname),
    model: (if $s then null else $in.model // null | ident end),
    round: $round,
    seconds: $seconds,
    stop_reason: ($in.stop_reason // null | ident),
    stop_category: ($in.stop_details.category? // null | ident),
    usage_scope: (if $s then "session" else "request" end),
    input_tokens: ($u.input_tokens | num),
    output_tokens: ($u.output_tokens | num),
    cache_read: (if $u == null then null elif $s then $u.cache_read | num else $u.cache_read_input_tokens // 0 | num end),
    cache_write: (if $u == null then null elif $s then $u.cache_write | num else $u.cache_creation_input_tokens // 0 | num end),
    list_cost: (if $s then $u.list_cost | cents else null end),
    advisor: (if ($a | length) > 0 then
                { model: ($a[0].model | ident), calls: ($a | length),
                  input_tokens: ($a | map(.input_tokens | num // 0) | add),
                  output_tokens: ($a | map(.output_tokens | num // 0) | add) }
              else null end),
    error: (if $error == "" then null else $error | ident end),
    # what the request let the model reach: the server tools by name, an MCP
    # server as mcp:<server> when it offers every tool, or one
    # mcp:<server>:<tool> per tool of an --mcp-tool allowlist. Recorded on a
    # failed request too: the tool set was decided before the reply.
    tools: ($ARGS.named.tools // null
             | if type == "array" then
                 [ .[] | objects
                   | if .type == "mcp_toolset" then
                       (.mcp_server_name // "?") as $srv
                       | if .default_config.enabled? == false then
                           (.configs // {} | to_entries[] | select(.value.enabled? == true) | "mcp:\($srv):\(.key)")
                         else "mcp:\($srv)" end
                     else .name // .type end
                   | toolname ]
               else null end),
    # how often the model used them in this reply: web searches and fetches as
    # usage.server_tool_use counts them, MCP calls as mcp_tool_use blocks.
    # Keyed on whether a reply arrived, not on $error: a stream that ends in an
    # error event still made, and was billed for, the calls it counts.
    tool_calls: (if $s or $in == null then null
                 else { web_search: ($u.server_tool_use.web_search_requests? | num),
                        web_fetch: ($u.server_tool_use.web_fetch_requests? | num),
                        mcp: ([$in.content[]? | objects | select(.type == "mcp_tool_use")] | length) } end) }

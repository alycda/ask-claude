# lib/request.jq: build the Messages API request body.
#
# Invocation (see tests/unit/request.bats), with claude.sh's argument names:
#     jq -n --arg m MODEL --argjson max N --arg sys SYSTEM --arg eff EFFORT \
#           --arg skills SKILLS_JSON --arg cid CONTAINER_ID \
#           --arg web 1-or-empty --arg mcp "https://a/mcp https://b/sse" \
#           --slurpfile msgs history.json -f lib/request.jq
#
# -n means there is no input: everything comes from the variables.
#   $m, $sys, $eff, $skills, $cid   strings; "" means "not given"
#   $max                            a number (--argjson parses it)
#   $msgs                           an ARRAY of the file's JSON values
#                                   (--slurpfile), so the history is $msgs[0]
#   $skills                         JSON text from lib/skills.jq; fromjson parses it
#   $web                            "" or anything: add the web search and fetch tools
#   $mcp                            MCP server URLs, space-separated; "" means none
#
# The tests compare with tests/fixtures/request/*.json byte for byte, so key
# order matters: `A + B` keeps A's keys first, then adds B's. Build the base
# object, then add each optional part only when its variable is not "".
#
# Shape:
#
#     BASE
#     + (if $sys != "" then {...} else {} end)
#     + ...

# BASE: every request has these six keys, in this order. Each value is either
# one of the variables or a constant:
#   model          $m           the model id (--arg m)
#   max_tokens     $max         a number, because --argjson parsed it
#   messages       $msgs[0]     the whole chat history. --slurpfile wraps the
#                               file in an array, so [0] unwraps it
#   fallbacks      "default"    let the API retry a refusal on another model
#   stream         true         always SSE (lib/stream.jq prints it)
#   cache_control  {type: "ephemeral"}   cache the growing conversation
def words: split(" ") | map(select(length > 0));
def host: capture("^[a-z]+://(?<h>[^/:]+)").h | gsub("[^A-Za-z0-9_-]"; "-");
# $web and $mcp are newer than the other arguments: read through $ARGS.named so
# a caller that does not pass them (the goldens of 0.1.0) still works
def web: $ARGS.named.web // "";
def mcp: $ARGS.named.mcp // "";

{
    model: $m,
    max_tokens: $max,
    messages: $msgs[0],
    fallbacks: "default",
    stream: true,
    cache_control: {type: "ephemeral"}
}
# system: the system prompt, only when one was given. When $sys is "" the
# else branch adds {}, which adds nothing, so the key is left out.
+ (if $sys != "" then {system: $sys} else {} end)
+ (if $eff != "" then {output_config: {effort: $eff}} else {} end)
# mcp_servers: one per URL in $mcp (space-separated), named after its host
# (dots to dashes), the token from the environment so it is never an argument
+ (if mcp != "" then {mcp_servers: [mcp | words[] | {type: "url", url: ., name: (. | host)}
                                      + (if (env.ASK_CLAUDE_MCP_TOKEN // "") != "" then {authorization_token: env.ASK_CLAUDE_MCP_TOKEN} else {} end)]}
   else {} end)
# tools, in this order: code_execution for skills, the web tools, one
# mcp_toolset per server; the key is left out when there are none
+ (([ (if $skills != "" then {type: "code_execution_20260521", name: "code_execution"} else empty end),
      (if web != "" then {type: "web_search_20250305", name: "web_search", max_uses: 5},
                           {type: "web_fetch_20250910", name: "web_fetch", max_uses: 5} else empty end) ]
    + (if mcp != "" then [mcp | words[] | {type: "mcp_toolset", mcp_server_name: (. | host)}] else [] end)) as $tools
   | if ($tools | length) > 0 then {tools: $tools} else {} end)
+ (if $skills != "" then
      {container: (({skills: ($skills | fromjson)}) + (if $cid != "" then {id: $cid} else {} end))}
    else {} end)

# lib/request.jq: build the Messages API request body.
#
# Invocation (see tests/unit/request.bats), with claude.sh's argument names:
#     jq -n --arg m MODEL --argjson max N --arg sys SYSTEM --arg eff EFFORT \
#           --arg skills SKILLS_JSON --arg cid CONTAINER_ID \
#           --arg web 1-or-empty --arg mcp "https://a/mcp https://b/sse" \
#           --arg advisor ADVISOR_MODEL --arg mcptools "a.example:search b.example:read" \
#           --slurpfile msgs history.json -f lib/request.jq
#
# -n means there is no input: everything comes from the variables.
#   $m, $sys, $eff, $skills, $cid   strings; "" means "not given"; $cid without
#                                   $skills goes out as the plain container id
#   $max                            a number (--argjson parses it)
#   $msgs                           an ARRAY of the file's JSON values
#                                   (--slurpfile), so the history is $msgs[0]
#   $skills                         JSON text from lib/skills.jq; fromjson parses it
#   $web                            "" or anything: add the web search and fetch tools
#                                   (the dynamic-filtering versions, or the basic ones
#                                   for a model older than Claude 4.6: basic_web)
#   $mcp                            MCP server URLs, space-separated; "" means none
#   $advisor                        the advisor tool's model; "" means no advisor
#   $mcptools                       --mcp-tool entries, host:tool, space-separated;
#                                   "" means every server offers all its tools
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
# the URL's hostname as written, lower-cased: what an --mcp-tool entry names
def hostname: capture("^[a-z]+://(?<h>[^/:]+)").h | ascii_downcase;
# $web, $mcp, $advisor and $mcptools are newer than the other arguments: read
# through $ARGS.named so a caller that does not pass them (the goldens of
# 0.1.0) still works
def web: $ARGS.named.web // "";
def mcp: $ARGS.named.mcp // "";
def advisor: $ARGS.named.advisor // "";
def mcptools: $ARGS.named.mcptools // "";
# basic_web: true for a model id older than Claude 4.6 (the 3.x family, and
# Opus, Sonnet and Haiku 4 through 4.5, with or without a date). Dynamic
# filtering needs 4.6 or later; on an older model the _20260318 web tools are
# a 400. Every alias and every other id gets the dynamic versions.
def basic_web: $m | test("\\Aclaude-3|\\Aclaude-(opus|sonnet|haiku)-4(-[0-5])?(-[0-9]{8})?\\z");
# toolset: the mcp_toolset for the server at this URL. With no --mcp-tool for
# its host the server offers every tool it lists. With one or more, it is an
# allowlist: every tool off by default, the named ones on. The script refuses
# an entry that names no --mcp host before it gets here, because a rule that
# matches nothing would read as a rule and allow everything.
def toolset:
  hostname as $h
  | [mcptools | words[] | select(split(":")[0] | ascii_downcase == $h) | sub("^[^:]*:"; "")] as $allow
  | {type: "mcp_toolset", mcp_server_name: host}
    + (if ($allow | length) > 0 then
         {default_config: {enabled: false}, configs: ($allow | map({key: ., value: {enabled: true}}) | from_entries)}
       else {} end);
# mcp_token: the authorization_token for the server at this URL. A token is
# one server's credential, so ASK_CLAUDE_MCP_TOKEN reaches a lone server
# only; each server takes ASK_CLAUDE_MCP_TOKEN_<HOST>, the host upper-cased
# with every other character an underscore, and only its own. Read from the
# environment, so a token is never an argument. The script refuses the
# shared token with several servers before it gets here.
def mcp_token:
  (env["ASK_CLAUDE_MCP_TOKEN_" + (host | ascii_upcase | gsub("-"; "_"))] // "") as $own
  | if $own != "" then $own
    elif (mcp | words | length) == 1 then env.ASK_CLAUDE_MCP_TOKEN // ""
    else "" end;
# the advisor calls already made this turn. A turn the API pauses (pause_turn)
# is continued with a new request, and each request carries its own max_uses,
# so the paused replies after the last user message count against the 3.
def advisor_used:
    $msgs[0] as $h
    | ([$h | to_entries[] | select(.value.role == "user") | .key] | last // -1) as $u
    | [$h[($u + 1):][] | .content | arrays | .[]
       | select(.type == "server_tool_use" and .name == "advisor")] | length;

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
# (dots to dashes), with its own token (mcp_token) when it has one
+ (if mcp != "" then {mcp_servers: [mcp | words[] | {type: "url", url: ., name: (. | host)}
                                      + (mcp_token as $t | if $t != "" then {authorization_token: $t} else {} end)]}
   else {} end)
# tools, in this order: code_execution for skills, the web tools (basic_web picks the version), one
# mcp_toolset per server (an allowlist when --mcp-tool names it), the advisor (3 calls a turn at most, less what this
# turn's paused replies used, and left out at 0: each call is a sub-inference
# at the advisor model's rates); the key is left out when there are none
+ (([ (if $skills != "" then {type: "code_execution_20260521", name: "code_execution"} else empty end),
      (if web == "" then empty
       elif basic_web then {type: "web_search_20250305", name: "web_search", max_uses: 5},
                           {type: "web_fetch_20250910", name: "web_fetch", max_uses: 5}
       # dynamic filtering: the model filters results in code before they
       # reach its context. The API provisions that code execution itself and
       # shares one container with the skills' code_execution (20260120 or
       # later is accepted alongside); not ZDR-eligible.
       else {type: "web_search_20260318", name: "web_search", max_uses: 5},
            {type: "web_fetch_20260318", name: "web_fetch", max_uses: 5} end) ]
    + (if mcp != "" then [mcp | words[] | toolset] else [] end)
    + (if advisor != "" and advisor_used < 3 then [{type: "advisor_20260301", name: "advisor", model: advisor, max_uses: (3 - advisor_used)}] else [] end)) as $tools
   | if ($tools | length) > 0 then {tools: $tools} else {} end)
+ (if $skills != "" then
      {container: (({skills: ($skills | fromjson)}) + (if $cid != "" then {id: $cid} else {} end))}
    # without skills, a container id is a pause_turn continuation's (the
    # caller sends one only then): the plain string form the API takes
    elif $cid != "" then {container: $cid}
    else {} end)

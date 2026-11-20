# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/local-mcp.bash: --local-mcp, a stdio MCP server on this machine that
# answers an agent's custom tools, over two FIFOs on descriptors 7 and 8.
#
# reads:   ASK_CLAUDE_LOCAL_MCP_TIMEOUT ASK_CLAUDE_VERSION; SAFE (common)
# sets:    LOCAL_MCP_PID LOCAL_MCP_DIR LOCAL_MCP_TOOLS LOCAL_MCP_TIMEOUT
#          (read by agent-turn), descriptors 7 and 8
# defines: local_mcp_send local_mcp_read local_mcp_start local_mcp_stop
#          local_mcp_alive local_mcp_has custom_error local_mcp_call
# calls:   die (ask-claude)

# --local-mcp: a stdio MCP server on this machine that answers the agent's
# custom tools. The main shell starts it once per session (session_answer runs
# in a command substitution, a subshell, so it could not keep a server it
# started) and talks to it over two FIFOs on fixed descriptors: 7 writes to
# the server, 8 reads from it. Subshells inherit both; `read -u 8` reads a
# byte at a time from a pipe, so a subshell never takes more than its line.
# Tool inputs and results reach jq on stdin only, never on a command line.
LOCAL_MCP_PID="" LOCAL_MCP_DIR="" LOCAL_MCP_TOOLS="[]"
LOCAL_MCP_TIMEOUT="${ASK_CLAUDE_LOCAL_MCP_TIMEOUT:-120}"

# local_mcp_send: write the JSON line on stdin to the server; fails, instead
# of killing the script with SIGPIPE, when the server has exited
local_mcp_send() {
  (
    trap '' PIPE
    cat >&7
  ) 2>/dev/null
}

# local_mcp_read <id>: print the server's response to <id>. Notifications and
# lines that are not JSON are skipped; a request from the server is answered
# with method-not-found, since ask-claude offers the server nothing. Returns
# 1 when the server has exited, 2 when it does not answer in time.
local_mcp_read() {
  local want=$1 line kind
  while :; do
    IFS= read -r -t "$LOCAL_MCP_TIMEOUT" -u 8 line
    case $? in
    0) ;;
    1) return 1 ;;
    *) return 2 ;;
    esac
    kind=$(jq -r --arg id "$want" '
      if type != "object" then "skip"
      elif has("method") then (if has("id") then "request" else "skip" end)
      elif (.id | tostring) == $id then "mine"
      else "skip" end' <<<"$line" 2>/dev/null) || kind=skip
    case "$kind" in
    mine)
      printf '%s\n' "$line"
      return 0
      ;;
    request) jq -c '{jsonrpc: "2.0", id: .id, error: {code: -32601, message: "ask-claude answers no server requests"}}' <<<"$line" | local_mcp_send ;;
    esac
  done
}

# local_mcp_start <command>: start the server with sh -c, run the handshake
# (initialize, notifications/initialized) and keep its tools/list in
# LOCAL_MCP_TOOLS. Dies, exit 2, when the server does not complete it.
local_mcp_start() {
  local cmd=$1 resp page=1 cursor=""
  case "$LOCAL_MCP_TIMEOUT" in
  "" | *[!0-9]* | 0*) die "ASK_CLAUDE_LOCAL_MCP_TIMEOUT must be whole seconds, got: $LOCAL_MCP_TIMEOUT" ;;
  esac
  LOCAL_MCP_DIR=$(mktemp -d) || die "cannot make a directory for the local MCP server's pipes"
  mkfifo "$LOCAL_MCP_DIR/in" "$LOCAL_MCP_DIR/out" || die "cannot make the local MCP server's pipes"
  # its own process group (job control on for this one job): sh forks a
  # command with a redirect instead of exec'ing it, and a server may start
  # children, so local_mcp_stop signals the group, not one pid. Ctrl-C at the
  # terminal reaches only the foreground group, so it does not hit the server.
  set -m
  sh -c "$cmd" <"$LOCAL_MCP_DIR/in" >"$LOCAL_MCP_DIR/out" &
  LOCAL_MCP_PID=$!
  set +m
  # the background shell opens in, then out, before it runs the command, so
  # these opens complete even when the command does not exist
  exec 7>"$LOCAL_MCP_DIR/in" 8<"$LOCAL_MCP_DIR/out"
  jq -nc --arg v "$ASK_CLAUDE_VERSION" '{jsonrpc: "2.0", id: "ask-claude-initialize", method: "initialize",
    params: {protocolVersion: "2025-06-18", capabilities: {}, clientInfo: {name: "ask-claude", version: $v}}}' | local_mcp_send
  resp=$(local_mcp_read ask-claude-initialize) || die "the local MCP server ($cmd) did not answer initialize"
  jq -e 'has("result")' <<<"$resp" >/dev/null ||
    die "the local MCP server ($cmd) refused initialize: $(jq -r "$SAFE"'.error.message // "no reason" | tostring | safe' <<<"$resp")"
  echo '{"jsonrpc":"2.0","method":"notifications/initialized"}' | local_mcp_send
  while :; do
    jq -nc --arg id "ask-claude-tools-$page" --arg c "$cursor" \
      '{jsonrpc: "2.0", id: $id, method: "tools/list"} + (if $c == "" then {} else {params: {cursor: $c}} end)' | local_mcp_send
    resp=$(local_mcp_read "ask-claude-tools-$page") || die "the local MCP server ($cmd) did not answer tools/list"
    jq -e '.result.tools | type == "array"' <<<"$resp" >/dev/null || die "the local MCP server ($cmd) sent no tools"
    LOCAL_MCP_TOOLS=$(printf '%s\n%s\n' "$LOCAL_MCP_TOOLS" "$resp" | jq -sc '.[0] + .[1].result.tools')
    cursor=$(jq -r '.result.nextCursor // empty | tostring' <<<"$resp")
    [ -n "$cursor" ] && [ "$page" -lt 20 ] || break
    page=$((page + 1))
  done
}

# local_mcp_stop: close the pipes (a stdio server exits at end of input);
# a group still running a second later gets SIGTERM, and a second after that
# SIGKILL
# shellcheck disable=SC2329  # called from the EXIT traps
local_mcp_stop() {
  [ -n "$LOCAL_MCP_PID" ] || return 0
  exec 7>&- 8<&-
  local _ sig
  for sig in TERM KILL; do
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      local_mcp_alive || break
      sleep 0.1
    done
    local_mcp_alive || break
    kill -s "$sig" -- "-$LOCAL_MCP_PID" 2>/dev/null
  done
  wait "$LOCAL_MCP_PID" 2>/dev/null
  rm -rf "$LOCAL_MCP_DIR"
  LOCAL_MCP_PID=""
}

# local_mcp_alive: a process of the server's group is running. A zombie is
# not: kill -0 counts one, and sh stays one until local_mcp_stop waits on it.
# shellcheck disable=SC2329  # called from local_mcp_stop, which the EXIT traps call
local_mcp_alive() {
  ps -A -o pgid= -o stat= 2>/dev/null | awk -v g="$LOCAL_MCP_PID" '$1 == g && $2 !~ /^Z/ { found = 1 } END { exit !found }'
}

# local_mcp_has <name>: the server lists a tool of that name
local_mcp_has() {
  [ -n "$LOCAL_MCP_PID" ] && jq -e --arg n "$1" 'any(.[]; .name == $n)' <<<"$LOCAL_MCP_TOOLS" >/dev/null
}

# custom_error <id> <text>: a user.custom_tool_result that reports an error
custom_error() {
  jq -nc --arg id "$1" --arg t "$2" '{type: "user.custom_tool_result", custom_tool_use_id: $id,
    content: [{type: "text", text: $t}], is_error: true}'
}

# local_mcp_call <custom tool use>: run it on the server as tools/call and
# print the user.custom_tool_result. Text content goes back as text, and so
# does an embedded text resource, under its URI (github-mcp-server returns a
# file that way). Binary resources, resource links, images and audio are
# named, not sent: the documented custom tool result is text. An empty
# result says so.
local_mcp_call() {
  local use=$1 id resp
  id=$(jq -r '.id' <<<"$use")
  if ! jq -c '{jsonrpc: "2.0", id: .id, method: "tools/call", params: {name: .name, arguments: (.input // {})}}' <<<"$use" | local_mcp_send; then
    custom_error "$id" "Tool failed: the local MCP server has exited."
    return
  fi
  resp=$(local_mcp_read "$id")
  case $? in
  0) ;;
  1)
    custom_error "$id" "Tool failed: the local MCP server has exited."
    return
    ;;
  *)
    custom_error "$id" "Tool failed: the local MCP server did not answer within ${LOCAL_MCP_TIMEOUT}s."
    return
    ;;
  esac
  jq -c --arg id "$id" '
    {type: "user.custom_tool_result", custom_tool_use_id: $id} +
    if has("error") then
      {content: [{type: "text", text: "Tool failed: MCP error \(.error.code // "?"): \(.error.message // "no message" | tostring)"}], is_error: true}
    else
      (.result // {}) as $r
      | {content: ([$r.content[]? | {type: "text", text: (
                     if .type == "text" then (.text | tostring)
                     elif .type == "resource" and (.resource.text | type) == "string" then "Resource \(.resource.uri // "(no uri)"):\n\(.resource.text)"
                     elif .type == "resource" then "[binary resource \(.resource.uri // "(no uri)") (\(.resource.mimeType // "unknown type")) not passed on by ask-claude]"
                     elif .type == "resource_link" then "Resource link: \(.uri // "(no uri)")" + (if .name then " (\(.name))" else "" end)
                     else "[\(.type // "unknown") content not passed on by ask-claude]" end)}]
                   | map(select(.text != ""))
                   | if length > 0 then .
                     elif $r.structuredContent then [{type: "text", text: ($r.structuredContent | tojson)}]
                     else [{type: "text", text: "(the tool returned no content)"}] end),
         is_error: ($r.isError == true)}
    end' <<<"$resp"
}

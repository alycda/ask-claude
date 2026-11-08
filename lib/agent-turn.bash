# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/agent-turn.bash: one session turn: open the stream, send, print,
# answer tool confirmations and custom tools, repeat until idle; the
# session loop and the checks every agent form runs first.
#
# reads:   usage_agent usage_environment usage_budget usage_file usage_vault
#          usage_repo usage_memory usage_local_mcp ASK_CLAUDE_REPLAY
#          ASK_CLAUDE_CONFIRM ASK_CLAUDE_VERSION GITHUB_TOKEN; LIB STATE
#          (ask-claude), BASE_URL (config), SAFE DOLLARS (common),
#          LOCAL_MCP_PID LOCAL_MCP_TOOLS (local-mcp)
# sets:    turn stream_pid ALWAYS session_tty session_interrupted, the INT
#          trap for a turn and the EXIT trap for the process
# defines: session_stream_open session_stream_read session_confirm
#          session_answer session_turn session_cleanup session_loop
#          session_prereqs
# calls:   ma_call ma_headers session_create session_remember
#          session_history (agent-api), local_mcp_*
#          custom_error (local-mcp), sse_data api_error error_kind event
#          (common), need_key (config), die (ask-claude)

# ---- Managed Agents sessions (docs/plans/2026-11-05-001) ----------------
# A session's transcript lives on the API; the state dir keeps one small file
# per session under sessions/ so `list` and `resume` know what this machine
# started. One turn is: send events, open the stream, print until the first
# session.status_idle, fold the stream, answer what the session waits on.

turn=0

# session_stream_open <sesn> <log>: start reading the session's stream into
# <log> before anything is sent, so no event is missed (a stream delivers only
# what is emitted after it opens). Sets stream_pid; waits for the stream to
# say it is connected. Replay: the fixture is the log, or turn N's file of a
# fixture directory, and there is nothing to wait for.
session_stream_open() {
  :
  stream_pid=""
  if [ -n "${ASK_CLAUDE_REPLAY:-}" ]; then
    if [ -d "$ASK_CLAUDE_REPLAY" ]; then cat "$ASK_CLAUDE_REPLAY/$turn.sse" >"$2"; else cat "$ASK_CLAUDE_REPLAY" >"$2"; fi
    return 0
  fi
  curl -sS -N --max-time 1800 "$BASE_URL/v1/sessions/$1/events/stream" -H @<(ma_headers) >"$2" 2>"$2.err" &
  stream_pid=$!
  for _ in $(seq 1 100); do
    grep -q '^: connected' "$2" 2>/dev/null && return 0
    kill -0 "$stream_pid" 2>/dev/null || return 0
    sleep 0.1
  done
}

# session_stream_read <log>: the stream's events as they land in <log>, until
# the first idle or termination; then the reader is done with curl
session_stream_read() {
  if [ -n "$stream_pid" ]; then
    tail -n +1 -f --pid="$stream_pid" "$1"
  else
    cat "$1"
  fi | sed -u '/"type":"session.status_idle"/q;/"type":"session.status_terminated"/q'
  [ -z "$stream_pid" ] || kill "$stream_pid" 2>/dev/null
  stream_pid=""
}

# session_confirm <name> <input>: allow | deny. A tool confirmation is asked
# at the terminal (y / n / a for the rest of this process) and denied when
# there is none; ASK_CLAUDE_CONFIRM=allow|deny answers without asking. The
# "always" answer is kept in a file, because this runs in a subshell; the
# file is $ALWAYS, which session_prereqs clears and removes on exit.
ALWAYS=""
session_confirm() {
  local answer
  case "${ASK_CLAUDE_CONFIRM:-ask}" in
  allow | deny)
    echo "$ASK_CLAUDE_CONFIRM"
    return
    ;;
  esac
  if [ -n "$ALWAYS" ] && [ -e "$ALWAYS" ]; then
    echo allow
    return
  fi
  if [ -z "$session_tty" ] || [ ! -r /dev/tty ]; then
    echo "ask-claude: denied $1 ($2): stdin is not a terminal (ASK_CLAUDE_CONFIRM=allow to allow unattended)" >&2
    echo deny
    return
  fi
  IFS= read -r -p "allow $1 $2? [y/N/a(lways)] " answer </dev/tty || answer=""
  case "$answer" in
  y | Y | yes) echo allow ;;
  a | A | always)
    [ -z "$ALWAYS" ] || : >"$ALWAYS"
    echo allow
    ;;
  *) echo deny ;;
  esac
}

# session_answer <fold>: the events that answer what the session waits on;
# nothing when it waits on something this version cannot answer
session_tty="" # set per answer round, outside the loop that redirects stdin
session_answer() {
  local fold=$1 answers="[]" ask id name shown input result use results=""
  if [ -t 0 ]; then session_tty=1; else session_tty=""; fi
  # One compact JSON line per ask (JSON escapes newlines, so a line is an
  # ask). The input is shown as jq -c renders it, encoded exactly once: going
  # through @tsv as well escaped its backslashes a second time, so a real
  # newline (two commands) looked like a literal \n (one command).
  while IFS= read -r ask; do
    id=$(jq -r '.id // empty' <<<"$ask")
    [ -n "$id" ] || continue
    # name and input go to the terminal (the prompt and the status lines):
    # safe, like every other API value printed; the id goes back unchanged
    name=$(jq -r "$SAFE"'.name | tostring | safe' <<<"$ask")
    input=$(jq -r "$SAFE"'.input | tojson | safe' <<<"$ask")
    result=$(session_confirm "$name" "$input")
    [ "$result" = allow ] && echo "ask-claude: allowed $name ($input)" >&2
    [ "$result" = deny ] && [ -n "${ASK_CLAUDE_CONFIRM:-}" ] && echo "ask-claude: denied $name ($input): ASK_CLAUDE_CONFIRM=$ASK_CLAUDE_CONFIRM" >&2
    answers=$(jq -c --arg id "$id" --arg r "$result" '. + [{type: "user.tool_confirmation", tool_use_id: $id, result: $r}]' <<<"$answers")
  done < <(jq -c '.asks[]' <<<"$fold")
  # Custom tools: run on the --local-mcp server when it lists the tool and
  # the call is allowed (the same confirmation as above: it runs on this
  # machine). The results carry tool output, so they reach jq on stdin.
  while IFS= read -r use; do
    id=$(jq -r '.id // empty' <<<"$use")
    [ -n "$id" ] || continue
    name=$(jq -r '.name | tostring' <<<"$use")
    if [ -z "$LOCAL_MCP_PID" ]; then
      results+="$(custom_error "$id" "Tool unavailable: ask-claude $ASK_CLAUDE_VERSION runs no local tools without --local-mcp. Do not call it again; answer the user without it.")"$'\n'
      continue
    fi
    if ! local_mcp_has "$name"; then
      results+="$(custom_error "$id" "Tool unavailable: the local MCP server has no tool named $name. Do not call it again; answer the user without it.")"$'\n'
      continue
    fi
    shown=$(jq -r "$SAFE"'.name | tostring | safe' <<<"$use")
    input=$(jq -r "$SAFE"'.input | tojson | safe' <<<"$use")
    result=$(session_confirm "local $shown" "$input")
    if [ "$result" = allow ]; then
      echo "ask-claude: allowed local $shown ($input)" >&2
      results+="$(local_mcp_call "$use")"$'\n'
    else
      [ -z "${ASK_CLAUDE_CONFIRM:-}" ] || echo "ask-claude: denied local $shown ($input): ASK_CLAUDE_CONFIRM=$ASK_CLAUDE_CONFIRM" >&2
      results+="$(custom_error "$id" "Tool call denied by the user: $name did not run.")"$'\n'
    fi
  done < <(jq -c '.custom[]' <<<"$fold")
  { printf '%s\n' "$answers"; printf '%s' "$results"; } | jq -sc '{events: (.[0] + .[1:])}'
}

# session_turn <sesn> [events]: one user turn, however many streams it takes.
# Prints the reply as it arrives and the usage line on stderr. Returns 1 on a
# session error, a terminated session, a transport failure, or a request the
# API refused; 130 after Ctrl-C, which sends user.interrupt so the session
# stops too.
session_interrupted=""
session_turn() {
  local sesn=$1 events=${2:-} log fold err stop rounds=0 t0=$SECONDS resp failure
  log="$STATE/.stream.$$"
  trap 'session_interrupted=1' INT
  while :; do
    turn=$((turn + 1)) # here, not in a pipeline: a subshell would not keep it
    : >"$log"
    session_stream_open "$sesn" "$log"
    if [ -n "$events" ]; then
      resp=$(ma_call POST "/sessions/$sesn/events" "$events")
      err=$(api_error "$resp")
      if [ -n "$err" ]; then
        echo "ask-claude: API error: $err" >&2
        # ma_call's own curl-failure body says no_response
        failure=$(error_kind <<<"$resp")
        echo null | event session $rounds $((SECONDS - t0)) "$failure" "$sesn"
        session_cleanup "$log"
        return 1
      fi
    fi
    session_stream_read "$log" | sse_data | jq --unbuffered -rj -f "$LIB/session.jq"
    if [ -n "$session_interrupted" ]; then
      echo "[interrupted: sending user.interrupt]" >&2
      echo null | event session $rounds $((SECONDS - t0)) interrupted "$sesn"
      ma_call POST "/sessions/$sesn/events" '{"events":[{"type":"user.interrupt"}]}' >/dev/null
      session_cleanup "$log"
      return 130
    fi
    if [ ! -s "$log" ] || ! grep -q '^data: ' "$log"; then
      echo "ask-claude: no response from the stream ($(head -c 200 "$log.err" "$log" 2>/dev/null | tr -d '\n' | jq -Rr "$SAFE"'safe' | cut -c1-200))" >&2
      echo null | event session $rounds $((SECONDS - t0)) no_response "$sesn"
      session_cleanup "$log"
      return 1
    fi
    if ! fold=$(sse_data <"$log" | jq -s -c -f "$LIB/session-fold.jq"); then
      echo "ask-claude: the stream could not be parsed" >&2
      echo null | event session $rounds $((SECONDS - t0)) parse "$sesn"
      session_cleanup "$log"
      return 1
    fi
    err=$(api_error "$fold")
    if [ -n "$err" ]; then
      echo "ask-claude: session error: $err" >&2
      event session $rounds $((SECONDS - t0)) "$(jq -r '.error.type // "unknown"' <<<"$fold")" "$sesn" <<<"$fold"
      session_cleanup "$log"
      return 1
    fi
    stop=$(jq -r "$SAFE"'.stop_reason // "" | tostring | safe' <<<"$fold")
    if [ "$stop" = requires_action ]; then
      if [ "$(jq '(.asks | length) + (.custom | length)' <<<"$fold")" -eq 0 ]; then
        echo "ask-claude: the session waits on something this version cannot answer ($(jq -r "$SAFE"'.event_ids | map(tostring) | join(" ") | safe' <<<"$fold")); interrupting" >&2
        event session $rounds $((SECONDS - t0)) unanswerable "$sesn" <<<"$fold"
        ma_call POST "/sessions/$sesn/events" '{"events":[{"type":"user.interrupt"}]}' >/dev/null
        session_cleanup "$log"
        return 1
      fi
      # custom tool calls are answered by this script, so they are capped;
      # confirmations are the user's, so they are not
      [ "$(jq '.asks | length' <<<"$fold")" -gt 0 ] || rounds=$((rounds + 1))
      if [ $rounds -le 8 ]; then
        events=$(session_answer "$fold")
        continue
      fi
    fi
    break
  done
  session_cleanup "$log"
  jq -r --arg s "$sesn" --argjson t $((SECONDS - t0)) "$SAFE$DOLLARS"'.usage
    | "[\($s) in=\(.input_tokens) out=\(.output_tokens) cache_read=\(.cache_read) cache_write=\(.cache_write) cost=\(.list_cost | dollars) \($t)s]" | safe' <<<"$fold" >&2
  # at the cap the last round was counted but not answered
  event session $((rounds > 8 ? 8 : rounds)) $((SECONDS - t0)) "" "$sesn" <<<"$fold"
  case "$stop" in
  end_turn | "") ;;
  requires_action)
    echo "[stopped: requires_action after $rounds custom tool rounds; interrupting]" >&2
    ma_call POST "/sessions/$sesn/events" '{"events":[{"type":"user.interrupt"}]}' >/dev/null
    return 1
    ;;
  terminated)
    echo "[stopped: the session was terminated]" >&2
    return 1
    ;;
  budget_reached) echo "[stopped: budget_reached, raise it with: ask-claude agent budget $sesn <cents>]" >&2 ;;
  *) echo "[stopped: $stop]" >&2 ;;
  esac
}

# session_cleanup <log>: the stream's files and process, and the Ctrl-C trap
session_cleanup() {
  [ -z "${stream_pid:-}" ] || kill "$stream_pid" 2>/dev/null
  stream_pid=""
  rm -f "$1" "$1.err"
  trap - INT
}

# session_loop [sesn]: one turn per line until /quit. With no session yet,
# the first line creates one (so quitting first leaves nothing); /new drops
# the current one and the next line creates another; /history prints the
# session so far.
session_loop() {
  local sesn=${1:-} line
  echo "ask-claude: agent $usage_agent  (/new starts another session, /quit exits)" >&2
  while IFS= read -r -e -p "> " line; do
    case "$line" in
    "") continue ;;
    /quit | /exit) break ;;
    /history)
      [ -n "$sesn" ] && session_history "$sesn"
      continue
      ;;
    /new)
      sesn=""
      echo "new session on the next line" >&2
      continue
      ;;
    esac
    if [ -z "$sesn" ]; then
      sesn=$(session_create "$line") || continue
      session_turn "$sesn" && session_remember "$sesn"
    else
      session_turn "$sesn" "$(printf '%s' "$line" | jq -n -c --rawfile t /dev/stdin '{events: [{type: "user.message", content: [{type: "text", text: $t}]}]}')"
    fi
    echo
  done
  [ -z "$sesn" ] || echo "resume with: ask-claude agent resume $sesn" >&2
}

# session_prereqs: what every agent form needs before a request can happen
session_prereqs() {
  # resources: checked before any request, the API's own checks come after
  local v n=0
  for v in ${usage_vault:-}; do
    [[ $v == vlt_* ]] || die "--vault takes vault ids (vlt_...), got: $v"
  done
  for v in ${usage_memory:-}; do
    [[ ${v%%:*} == memstore_* ]] || die "--memory takes memory store ids (memstore_...), with :ro for read-only, got: $v"
    n=$((n + 1))
  done
  [ "$n" -le 8 ] || die "--memory: a session takes at most 8 memory stores, got $n"
  if [ -n "${usage_repo:-}" ]; then
    [[ $usage_repo =~ ^(https://github\.com/)?[^/@]+/[^/@]+(@.+)?$ ]] || die "--repo takes owner/name[@branch] or a https://github.com URL, got: $usage_repo"
    [ -n "${GITHUB_TOKEN:-}" ] || die "--repo needs GITHUB_TOKEN in the environment (a token that can read $usage_repo)"
  fi
  [ -n "${usage_agent:-}" ] || die "no agent id: --agent or ASK_CLAUDE_AGENT (the Console, Agents)"
  [ -n "${usage_environment:-}" ] || die "no environment id: --environment or ASK_CLAUDE_ENVIRONMENT (the Console, Environments)"
  case "${usage_budget:-}" in
  "" | *[!0-9]*) die "--budget must be whole cents, 0 for none, got: ${usage_budget:-}" ;;
  esac
  case "${ASK_CLAUDE_CONFIRM:-ask}" in
  ask | allow | deny) ;;
  *) die "ASK_CLAUDE_CONFIRM must be ask, allow or deny, got: $ASK_CLAUDE_CONFIRM" ;;
  esac
  [ -z "${usage_file:-}" ] || [ -r "$usage_file" ] || die "cannot read $usage_file"
  need_key
  # An "always" answer lasts for this process only. The file is keyed by the
  # PID, and PIDs are reused (deterministically, in a container that starts
  # the same way each time): a file left by an earlier process with this PID,
  # killed before its EXIT trap ran, must not answer for this one.
  ALWAYS="$STATE/.always.$$"
  rm -f "$ALWAYS"
  trap 'rm -f "$ALWAYS"; local_mcp_stop' EXIT
  # last, so a bad flag or a missing key fails before a server starts
  [ -z "${usage_local_mcp:-}" ] || local_mcp_start "$usage_local_mcp"
}

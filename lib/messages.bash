# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/messages.bash: one turn on the Messages API: build the request from
# the history, stream the reply, fold it back into the history.
#
# reads:   usage_model usage_max_tokens usage_system usage_effort usage_web
#          usage_mcp usage_mcp_tool usage_advisor ASK_CLAUDE_REPLAY ASK_CLAUDE_REQUEST_LOG
#          ASK_CLAUDE_BETA_LOG; LIB (ask-claude), API (config), SAFE
#          (common), HIST (ask-claude, chat)
# sets:    betas (call sets it, transport reads it: a global, not an argument),
#          WEB_NOTICE_SHOWN (the --web ZDR notice, once per run)
#          and the files $HIST, $HIST.container, $HIST.sse, $HIST.turn
# defines: transport call ask
# calls:   skills_json rollback_to (chat), sse_data api_error error_kind
#          event headers (common)

# transport: the request body on stdin, the raw SSE on stdout.
# ASK_CLAUDE_REPLAY=<file.sse> replays a recording instead of calling curl;
# ASK_CLAUDE_REQUEST_LOG=<file> appends each body that would have been sent,
# and ASK_CLAUDE_BETA_LOG=<file> each request's anthropic-beta value, so a
# replay test can see the header a recording never shows.
# Neither the key nor the body goes on curl's command line, which other local
# accounts can read: the headers come from a pipe and the body from stdin.
transport() {
  local body
  body=$(cat)
  [ -z "${ASK_CLAUDE_BETA_LOG:-}" ] || printf '%s\n' "$betas" >>"$ASK_CLAUDE_BETA_LOG"
  # The log line is written before anything is sent, as the sessions path
  # does it. The old `tee -a LOG | cat REPLAY` lost the line whenever the
  # replay side had already exited: GNU tee writes to the pipe first and dies
  # on the SIGPIPE before it reaches the file. BSD tee ignores the signal.
  # A log that cannot be written fails the request, as the tee inside the
  # pipefail pipeline did; a test that reads the log then sees this message
  # instead of jq failing on a file that is not there.
  if [ -n "${ASK_CLAUDE_REQUEST_LOG:-}" ] && ! printf '%s
' "$body" >>"$ASK_CLAUDE_REQUEST_LOG"; then
    echo "ask-claude: the request log $ASK_CLAUDE_REQUEST_LOG cannot be written; nothing sent" >&2
    return 1
  fi
  if [ -n "${ASK_CLAUDE_REPLAY:-}" ]; then
    cat "$ASK_CLAUDE_REPLAY"
  else
    printf '%s' "$body" | curl -sS -N --max-time 1800 "$API" \
      -H @<(headers "$betas") \
      --data-binary @-
  fi
}

# call: send the whole history as-is and append the reply. Prints the text
# blocks as they arrive, the usage line on stderr; returns 1 on API error
# with the history rolled back to before this ask. A pause_turn (server tool
# still working) is continued automatically, up to 8 rounds.
call() {
  local body resp err stop rounds=0 n0 log t0 skills cid failure chat rtools turn_cid=""
  n0=$(jq length "$HIST")
  chat=$(basename "$HIST" .json)
  # Skills run in Anthropic's code-execution sandbox, so -s also adds the
  # code_execution tool and its beta; the container id comes back in the
  # reply and is kept in $HIST.container so files persist across turns.
  skills=$(skills_json)
  betas="server-side-fallback-2026-07-01"
  [ -n "$skills" ] && betas="$betas,code-execution-2025-08-25"
  [ -z "${usage_mcp:-}" ] || betas="$betas,mcp-client-2025-11-20"
  # the advisor beta also when the history holds advisor blocks, e.g. a chat
  # resumed without --advisor: the API refuses those blocks without it
  if [ -n "${usage_advisor:-}" ] || grep -q '"advisor_tool_result"' "$HIST"; then
    betas="$betas,advisor-tool-2026-03-01"
  fi
  while :; do
    # the container to run in. With skills, the chat's container, so files
    # persist across turns. Without, only a pause_turn continuation sends one
    # back: the container the previous round of this turn ran in (dynamic web
    # filtering runs code there), so the paused work continues where it was
    # instead of in a new container. A new turn starts without one.
    cid=""
    if [ -n "$skills" ]; then
      [ -s "$HIST.container" ] && cid=$(cat "$HIST.container")
    else
      cid=$turn_cid
    fi
    body=$(jq -n -c --arg m "$usage_model" --argjson max "$usage_max_tokens" \
      --arg sys "${usage_system:-}" --arg eff "${usage_effort:-}" \
      --arg skills "$skills" --arg cid "$cid" --arg web "${usage_web:+1}" --arg mcp "${usage_mcp:-}" --arg advisor "${usage_advisor:-}" --arg mcptools "${usage_mcp_tool:-}" \
      --slurpfile msgs "$HIST" -f "$LIB/request.jq")
    # the tools this request enables, for the event log: what the model could
    # reach, recorded on every line of the request, failed ones included
    rtools=$(printf '%s' "$body" | jq -c '.tools // null')
    # once per run: the dynamic-filtering web tools are not ZDR-eligible, and
    # --web sends them for every model, so say so where it is seen
    if [ -z "${WEB_NOTICE_SHOWN:-}" ] && jq -e 'any(.[]?; .type == "web_search_20260318")' <<<"$rtools" >/dev/null 2>&1; then
      echo "ask-claude: --web uses the dynamic-filtering web tools (web_search_20260318, web_fetch_20260318), which are not eligible for Zero Data Retention" >&2
      WEB_NOTICE_SHOWN=1
    fi
    log="$HIST.sse" t0=$SECONDS
    : >"$log"
    # tee the raw SSE to $log, strip "data: " with builtins, print text live
    printf '%s\n' "$body" | transport |
      tee "$log" |
      sse_data |
      jq --unbuffered -rj -f "$LIB/stream.jq"
    [ -s "$log" ] || {
      rm -f "$log"
      echo "ask-claude: no response (curl failed or timed out)" >&2
      echo null | event messages $rounds $((SECONDS - t0)) no_response "$chat" "$rtools"
      rollback_to $((n0 - 1))
      return 1
    }
    # each arm names the failure for the event log: the API's error.type when
    # there is one, never its message
    resp=null
    if ! grep -q '^data: ' "$log"; then
      # not SSE: an HTTP error body
      err=$(api_error "$(cat "$log")")
      [ -n "$err" ] || err="unreadable response"
      failure=$(error_kind <"$log")
    elif ! resp=$(sse_data <"$log" | jq -s -f "$LIB/reassemble.jq"); then
      # a reply jq cannot fold is an error, not an empty turn that vanishes
      err="the reply could not be parsed"
      resp=null failure=parse
    else
      err=$(printf '%s' "$resp" | jq -r "$SAFE"'.error.message // empty | safe')
      [ -z "$err" ] || failure=$(printf '%s' "$resp" | error_kind)
    fi
    if [ -n "$err" ]; then
      rm -f "$log"
      echo "ask-claude: API error: $err" >&2
      printf '%s' "$resp" | event messages $rounds $((SECONDS - t0)) "$failure" "$chat" "$rtools"
      # a container the API no longer accepts (expired, deleted) would fail
      # every later turn of this chat: forget it, the next turn starts a new one
      # only the skills container is a file: a pause_turn continuation's
      # in-memory id (no -s) must not delete the chat's skills container
      [ -n "$skills" ] && [ -n "$cid" ] && case "$err" in *[Cc]ontainer*) rm -f "$HIST.container" ;; esac
      rollback_to $((n0 - 1))
      return 1
    fi
    # keep the full content array (thinking blocks included) for the next turn
    printf '%s' "$resp" | jq '{role:"assistant", content:.content}' >"$HIST.turn"
    jq --slurpfile t "$HIST.turn" '. + $t' "$HIST" >"$HIST.tmp" && mv "$HIST.tmp" "$HIST" && rm -f "$HIST.turn"
    turn_cid=$(printf '%s' "$resp" | jq -r '.container.id // empty')
    # $HIST.container is the skills container, kept across turns. A reply
    # without -s can carry a container too (dynamic web filtering), and
    # writing it here would replace the skills container of a chat that used
    # -s earlier, orphaning its files: that id lives in turn_cid alone.
    if [ -n "$skills" ]; then
      printf '%s' "$turn_cid" >"$HIST.container.tmp"
      if [ -s "$HIST.container.tmp" ]; then mv "$HIST.container.tmp" "$HIST.container"; else rm -f "$HIST.container.tmp"; fi
    fi
    rm -f "$log"
    stop=$(printf '%s' "$resp" | jq -r "$SAFE"'.stop_reason | safe')
    # the top-level usage is the executor's alone; an advisor's tokens are in
    # usage.iterations, billed at its own rates, so name them on the line too
    printf '%s' "$resp" | jq -r --argjson s $((SECONDS - t0)) "$SAFE"'
      ([.usage.iterations[]? | select(.type == "advisor_message")]) as $a
      | (if ($a | length) > 0 then " advisor=\($a[0].model) in=\($a | map(.input_tokens // 0) | add) out=\($a | map(.output_tokens // 0) | add)" else "" end) as $adv
      | "[\(.model) in=\(.usage.input_tokens) cache_read=\(.usage.cache_read_input_tokens // 0) cache_write=\(.usage.cache_creation_input_tokens // 0) out=\(.usage.output_tokens)\($adv) \($s)s]" | safe' >&2
    printf '%s' "$resp" | event messages $rounds $((SECONDS - t0)) "" "$chat" "$rtools"
    if [ "$stop" = "pause_turn" ] && [ $rounds -lt 8 ]; then
      rounds=$((rounds + 1))
      echo "[pause_turn: continuing, round $rounds]" >&2
      continue
    fi
    break
  done
  case "$stop" in
  end_turn) ;;
  refusal) echo "[stopped: refusal $(printf '%s' "$resp" | jq -r "$SAFE"'.stop_details.category // "" | safe')]" >&2 ;;
  max_tokens) echo "[stopped: max_tokens reached, raise --max-tokens]" >&2 ;;
  *) echo "[stopped: $stop]" >&2 ;;
  esac
}

# ask <text>: append a user turn, then call. The text (a typed line, or a
# whole -f file) reaches jq on stdin: a command line is readable by other
# local accounts, the same reason transport keeps the body off curl's.
ask() {
  printf '%s' "$1" | jq --rawfile c /dev/stdin '. + [{"role":"user","content":$c}]' "$HIST" >"$HIST.tmp" && mv "$HIST.tmp" "$HIST"
  call
}

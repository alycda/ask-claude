# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/agent-api.bash: Managed Agents requests that are one call each:
# create, find, list, history, files, budget, archive; and the sessions/
# directory that remembers what this machine started.
#
# reads:   usage_agent usage_environment usage_budget usage_file usage_vault
#          usage_repo usage_memory usage_force ASK_CLAUDE_REPLAY
#          ASK_CLAUDE_REQUEST_LOG ANTHROPIC_API_KEY; LIB STATE
#          session_file_id (ask-claude), BASE_URL (config), SAFE DOLLARS
#          (common)
# sets:    SESSIONS MA_BETA (read by agent-turn)
# defines: ma_headers ma_call session_create session_id_ok session_remember
#          session_find session_history session_list file_upload
#          session_files session_budget session_archive session_api_error
# calls:   headers api_error error_kind event (common), need_key (config),
#          die (ask-claude)

SESSIONS="$STATE/sessions"
MA_BETA="managed-agents-2026-04-01"

ma_headers() { headers "$MA_BETA"; }

# ma_call <method> <path> [body]: one request, the response body on stdout;
# a transport failure prints an error body so callers see it through
# api_error. Replay: the body is logged (GITHUB_TOKEN included, so a test can
# see it was sent) and a canned response stands in for the API.
ma_call() {
  local method=$1 path=$2 body=${3:-}
  [ -z "${ASK_CLAUDE_REQUEST_LOG:-}" ] || [ -z "$body" ] || printf '%s\n' "$body" >>"$ASK_CLAUDE_REQUEST_LOG"
  if [ -n "${ASK_CLAUDE_REPLAY:-}" ]; then
    case "$method $path" in
    "POST /sessions") cat "$ASK_CLAUDE_REPLAY/create.json" 2>/dev/null || echo '{"id":"sesn_FIXTURE01","status":"running"}' ;;
    "POST /sessions/"*"/archive") cat "$ASK_CLAUDE_REPLAY/archive.json" 2>/dev/null || echo '{"id":"sesn_FIXTURE01","status":"terminated","archived_at":"2026-11-05T00:00:00Z"}' ;;
    "POST /sessions/"*"/events") cat "$ASK_CLAUDE_REPLAY/events-post.json" 2>/dev/null || echo '{}' ;;
    "POST /sessions/"*) cat "$ASK_CLAUDE_REPLAY/update.json" 2>/dev/null || jq -c '{id: "sesn_FIXTURE01"} + .' <<<"$body" ;;
    "GET /sessions/"*"/events"*) cat "$ASK_CLAUDE_REPLAY/events.json" 2>/dev/null || echo '{"data":[]}' ;;
    "GET /sessions"*) cat "$ASK_CLAUDE_REPLAY/sessions.json" 2>/dev/null || echo '{"data":[]}' ;;
    "GET /files"*) cat "$ASK_CLAUDE_REPLAY/files.json" 2>/dev/null || echo '{"data":[]}' ;;
    *) echo '{}' ;;
    esac
    return 0
  fi
  if [ -n "$body" ]; then
    printf '%s' "$body" | curl -sS -g --max-time 120 -X "$method" "$BASE_URL/v1$path" -H @<(ma_headers) --data-binary @- ||
      echo '{"error":{"type":"no_response","message":"no response (curl failed or timed out)"}}'
  else
    curl -sS -g --max-time 120 -X "$method" "$BASE_URL/v1$path" -H @<(ma_headers) ||
      echo '{"error":{"type":"no_response","message":"no response (curl failed or timed out)"}}'
  fi
}

# session_create <prompt>: POST /v1/sessions with the prompt as the first
# event; prints the new session id, or an API error and returns 1.
session_create() {
  local body resp sesn err failure t0=$SECONDS
  # the prompt on stdin, not on jq's command line (see ask)
  body=$(printf '%s' "$1" | jq -n -c --arg agent "$usage_agent" --arg env "$usage_environment" --arg budget "$usage_budget" --arg file "${session_file_id:-}" --arg file_name "$(basename "${usage_file:-}")" \
    --arg vaults "${usage_vault:-}" --arg repo "${usage_repo:-}" --arg memories "${usage_memory:-}" \
    --arg title "" --rawfile prompt /dev/stdin -f "$LIB/session-create.jq")
  resp=$(ma_call POST /sessions "$body")
  sesn=$(jq -r '.id // empty' <<<"$resp" 2>/dev/null)
  if [ -z "$sesn" ]; then
    err=$(api_error "$resp")
    [ -n "$err" ] || err="unreadable response"
    echo "ask-claude: API error: $err" >&2
    failure=$(error_kind <<<"$resp")
    echo null | event session 0 $((SECONDS - t0)) "$failure" ""
    return 1
  fi
  # the id names a file under sessions/ and goes into URL paths and the
  # terminal, so it must be an id and nothing else ("../x" would write
  # outside sessions/)
  if ! session_id_ok "$sesn"; then
    echo "ask-claude: API error: the session id is not one (sesn_ and letters, digits, _ or -)" >&2
    echo null | event session 0 $((SECONDS - t0)) unreadable ""
    return 1
  fi
  echo "$sesn"
}

# session_id_ok <id>: the shape of a session id the API hands out
session_id_ok() { [[ $1 =~ ^sesn_[A-Za-z0-9_-]{1,128}$ ]]; }

# session_remember <sesn>: the state file `list` and `resume` read
session_remember() {
  session_id_ok "$1" || return 1
  mkdir -p "$SESSIONS"
  printf 'agent=%s\nenvironment=%s\ncreated=%s\n' "$usage_agent" "$usage_environment" "$(date +%Y%m%dT%H%M%S)" >"$SESSIONS/$1"
}

# session_find <id-or-prefix>: a session id, from sessions/ by prefix, or the
# id itself when it looks like one; exit 2 otherwise
session_find() {
  local f n=0 found="" pat
  case "$1" in */* | *[*?[]*) die "resume takes a session id or a prefix from \`ask-claude agent list\`" ;; esac
  # a prefix with or without the sesn_ part; one glob, so a match counts once
  case "$1" in sesn_*) pat="$SESSIONS/$1" ;; *) pat="$SESSIONS/sesn_$1" ;; esac
  for f in "$pat"*; do
    [ -f "$f" ] || continue
    found="${f##*/}"
    n=$((n + 1))
  done
  if [ "$n" -gt 1 ]; then
    die "\"$1\" matches $n sessions, see \`ask-claude agent list\`"
  elif [ "$n" -eq 1 ]; then
    echo "$found"
  elif [[ $1 == sesn_* ]]; then
    echo "$1"
  else
    die "nothing to resume: \"$1\" is not a session id, nor a prefix of one started here"
  fi
}

# session_history <sesn>: the session's messages so far, from the API
session_history() {
  local page
  page=$(ma_call GET "/sessions/$1/events?types[]=user.message&types[]=agent.message")
  session_api_error "$page" && return 1
  jq -r -f "$LIB/session-history.jq" <<<"$page"
}

# session_list: the API's sessions, newest first
session_list() {
  local page
  page=$(ma_call GET "/sessions?order=desc&limit=50")
  session_api_error "$page" && return 1
  jq -r "$SAFE$DOLLARS"'.data[]? | [.id, .status, ((.usage.list_cost.amount // "0") | dollars), (.title // "")] | join("  ") | safe' <<<"$page"
}

# file_upload <path>: POST /v1/files (multipart, no beta header: the Files
# API is out of beta); prints the file id. Replay logs {"file": name}.
file_upload() {
  local resp id name
  name=$(basename "$1")
  if [ -n "${ASK_CLAUDE_REPLAY:-}" ]; then
    [ -z "${ASK_CLAUDE_REQUEST_LOG:-}" ] || jq -n -c --arg f "$name" '{file: $f}' >>"$ASK_CLAUDE_REQUEST_LOG"
    echo file_FIXTURE01
    return 0
  fi
  resp=$(curl -sS "$BASE_URL/v1/files" -H @<(printf 'x-api-key: %s\nanthropic-version: 2026-10-21\n' "$ANTHROPIC_API_KEY") -F "file=@$1")
  id=$(jq -r '.id // empty' <<<"$resp" 2>/dev/null)
  if [ -z "$id" ]; then
    echo "ask-claude: upload failed: $(api_error "$resp")" >&2
    return 1
  fi
  echo "$id"
}

# session_files <sesn> [name]...: the files a session holds (uploads and what
# the agent wrote to /mnt/session/outputs), or download the named ones into
# the current directory
session_files() {
  local sesn=$1 page name id size tmp
  shift
  page=$(ma_call GET "/files?scope_id=$sesn")
  session_api_error "$page" && return 1
  if [ $# -eq 0 ]; then
    # Aligned here, not by column(1): minimal Debian and Ubuntu images do not
    # ship it. Same layout as `column -t`: each column but the last padded to
    # its widest cell, two spaces between. Each cell is escaped as @tsv
    # escapes it (tab, newline, backslash), then made safe. Width is counted
    # in code points, so a name with wide characters can push its row over.
    jq -r "$SAFE"'
      [.data[]? | [.filename, .size_bytes, .id] | map([.] | @tsv | safe)] as $rows
      | ($rows | map(map(length)) | transpose | map(max)) as $w
      | $rows[]
      | [to_entries[] | if .key < ($w | length) - 1 then .value + " " * ($w[.key] - (.value | length) + 1) else .value end]
      | join(" ")' <<<"$page" 2>/dev/null ||
      { echo "ask-claude: the session's file list is not in the shape the script reads" >&2; return 1; }
    return 0
  fi
  for name in "$@"; do
    case "$name" in */* | "" | .* | -*) die "files takes names from \`ask-claude agent files $sesn\`, not paths" ;; esac
    id=$(jq -r --arg n "$name" 'last(.data[]? | select(.filename == $n)) | .id // empty' <<<"$page")
    [ -n "$id" ] || die "the session has no file named $name"
    # --force replaces a file, never a link or a directory (mv would follow
    # the one and move into the other)
    if [ -L "$name" ] || { [ -e "$name" ] && [ ! -f "$name" ]; }; then
      die "$name exists here and is not a regular file; not overwriting it"
    fi
    if [ -e "$name" ] && [ "${usage_force:-}" != true ]; then
      die "$name exists here; --force overwrites it"
    fi
    # The bytes go to a temp file this process creates (mktemp: O_EXCL), so
    # nothing already in the directory, such as a planted symlink, can
    # redirect them; then one rename puts the file in place.
    tmp=$(mktemp "./.$name.XXXXXX") || die "cannot create a temporary file in $PWD"
    if [ -n "${ASK_CLAUDE_REPLAY:-}" ]; then
      cat "$ASK_CLAUDE_REPLAY/files/$name" >"$tmp"
    elif ! curl -sS --fail --max-time 600 "$BASE_URL/v1/files/$id/content" -H @<(ma_headers) -o "$tmp"; then
      rm -f "$tmp"
      echo "ask-claude: download of $name failed" >&2
      return 1
    fi
    mv -f -- "$tmp" "$name"
    size=$(wc -c <"$name")
    echo "ask-claude: wrote $name ($size bytes)" >&2
  done
}

# session_budget <sesn> <cents>: change the cap; 0 removes it, which cannot
# be undone (the API says so). A cap below what is spent is refused by the
# API with its own message.
session_budget() {
  local body resp
  case "$2" in
  "" | *[!0-9]*) die "budget takes whole cents, 0 to remove the cap, got: $2" ;;
  esac
  need_key
  if [ "$2" = 0 ]; then body='{"budget":null}'; else body=$(jq -n -c --arg c "$2" '{budget: {type: "limit", max_list_cost: {amount: $c, currency: "USD"}}}'); fi
  resp=$(ma_call POST "/sessions/$1" "$body")
  session_api_error "$resp" && return 1
  jq -r "$SAFE$DOLLARS"'"budget: " + (if .budget == null then "none" else (.budget.max_list_cost.amount | dollars) end) | safe' <<<"$resp"
}

# session_archive <sesn>: archive an idle session (the API refuses a running
# one) and forget it here
session_archive() {
  local resp
  need_key
  resp=$(ma_call POST "/sessions/$1/archive")
  session_api_error "$resp" && return 1
  rm -f "$SESSIONS/$1"
  echo "archived $1"
}

# session_api_error <response>: print an API error and succeed if there is one
session_api_error() {
  local err
  err=$(api_error "$1")
  [ -n "$err" ] || return 1
  echo "ask-claude: API error: $err" >&2
  return 0
}

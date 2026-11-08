# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/common.bash: what both the Messages API and the sessions use: the
# terminal-safe jq prefixes, the event log, error bodies, request headers,
# SSE lines.
#
# reads:   ASK_CLAUDE_EVENTS ANTHROPIC_API_KEY ASK_CLAUDE_VERSION;
#          LIB STATE (ask-claude), HOST (config)
# sets:    SAFE DOLLARS (read by every module that prints API text)
# defines: event error_kind headers api_error sse_data

# SAFE: prefix for every jq program that prints text to the terminal (the
# live printer has its own copy in lib/stream.jq). Replies and error messages
# may carry control characters; jq -r decodes \u001b etc. into raw bytes.
SAFE='def safe: if type == "string" then explode | map(select(. == 9 or . == 10 or (. >= 32 and . != 127 and (. < 128 or . > 159)))) | implode else . end; '
# DOLLARS: cents as the API gives them (a string) to "$1.23", for the usage
# line, `agent list` and `agent budget`
# shellcheck disable=SC2016  # a jq program, not a shell string
# shellcheck disable=SC2034  # read by lib/agent-api.bash and lib/agent-turn.bash
DOLLARS='def dollars: (tonumber / 100) as $d | "$" + ($d | floor | tostring) + "." + (($d * 100 | round) % 100 | tostring | if length < 2 then "0" + . else . end); '

# event <kind> <round> <seconds> <error> <chat>: append one line to the event
# log, built by lib/event.jq from the message, fold or null on stdin. Tokens,
# timing and outcome only, never prompt, reply or error text. Off with
# ASK_CLAUDE_EVENTS=off. A log that cannot be written is skipped in silence:
# the redirect sits inside the group, because bash reports a failed >> open
# before a trailing 2>/dev/null applies.
event() {
  local line
  [ "${ASK_CLAUDE_EVENTS:-}" != off ] || return 0
  line=$(jq -c --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg version "$ASK_CLAUDE_VERSION" \
    --arg kind "$1" --argjson round "$2" --argjson seconds "$3" --arg error "$4" \
    --arg chat "$5" --arg host "$HOST" -f "$LIB/event.jq" 2>/dev/null) || return 0
  # in a subshell: a signal from the write (SIGXFSZ at a ulimit -f) ends it,
  # not the script
  (printf "%s\n" "$line" >>"$STATE/events.jsonl") 2>/dev/null || :
}

# error_kind: the event-log category for an error body on stdin: its
# error.type, unknown for JSON without one, unreadable for anything else
error_kind() { jq -rs 'if (.[0] | type) == "object" then .[0].error.type // "unknown" else "unreadable" end' 2>/dev/null || echo unreadable; }

# headers <beta>: the request headers for a header file on a pipe
headers() { printf 'content-type: application/json\nx-api-key: %s\nanthropic-version: 2026-10-21\nanthropic-beta: %s\n' "$ANTHROPIC_API_KEY" "$1"; }

# api_error <json>: the API's error message, "unreadable response" for a body
# that is not JSON, nothing for a body without .error
api_error() { jq -r "$SAFE"'.error.message // empty | safe' <<<"$1" 2>/dev/null || echo "unreadable response"; }

# sse_data: the JSON of every "data: " line, one per line (a shell loop, so
# one jq handles the whole stream instead of one per event)
sse_data() { while IFS= read -r l; do case "$l" in "data: "*) printf '%s\n' "${l#data: }" ;; esac done }

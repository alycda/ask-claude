# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/chat.bash: the chats on disk under $STATE, one JSON history per chat,
# and the chat loop.
#
# reads:   usage_model usage_skills; LIB STATE (ask-claude), SAFE (common)
# sets:    HIST (chat_loop's /new; ask-claude sets it first)
# defines: new_chat rollback_to skills_json show_history list_chats
#          find_chat chat_loop
# calls:   ask (messages), die (ask-claude)

# A fresh history path; the PID keeps two processes started in the same second
# apart.
new_chat() {
  local f
  f="$STATE/$(date +%Y%m%dT%H%M%S)-$$.json"
  # /new in the same second would reuse the name and overwrite the chat
  while [ -e "$f" ]; do
    sleep 1
    f="$STATE/$(date +%Y%m%dT%H%M%S)-$$.json"
  done
  echo "$f"
}

# rollback_to <n>: keep the first n turns of the history (n >= 0)
rollback_to() { jq --argjson n "$(($1 < 0 ? 0 : $1))" '.[:$n]' "$HIST" >"$HIST.tmp" && mv "$HIST.tmp" "$HIST"; }

# skills_json: the container.skills array from -s ("" when not given), one
# typed entry per <skill_id>[@<version>] (lib/skills.jq)
skills_json() {
  [ -n "${usage_skills:-}" ] || {
    echo ""
    return
  }
  printf '%s' "$usage_skills" | tr ',' '\n' | jq -R -s -c -f "$LIB/skills.jq"
}

# show_history [file]: the chat as "you:" / "claude:" text (thinking blocks skipped)
show_history() {
  jq -r "$SAFE"'.[] | (if .role=="user" then "you" else "claude" end) as $who
        | (if (.content|type)=="string" then .content
           else [.content[] | select(.type=="text") | .text] | join("\n") end) as $t
        | "\($who): \($t)\n" | safe' "${1:-$HIST}"
}

# list_chats: chats on disk, newest first: name, turn count, the first
# prompt cut at 60 characters
list_chats() {
  # shellcheck disable=SC2012  # ls -t is the sort; names are ours, one per line
  ls -1t "$STATE"/*.json 2>/dev/null | while IFS= read -r f; do
    printf '%s  %s turns  %s\n' "$(basename "$f" .json)" "$(jq length "$f")" \
      "$(jq -r "$SAFE"'first(.[]|select(.role=="user"))|.content|if type=="string" then .[:60] else "" end | safe' "$f")"
  done
}

# find_chat [name-or-prefix]: the history file to resume, or exit 2. With no
# argument, the newest chat that has turns (empty files from aborted starts
# are skipped).
find_chat() {
  local f n=0 found=""
  if [ -n "${1:-}" ]; then
    # a chat name or prefix from `list`: no path parts or glob characters
    case "$1" in */* | *[*?[]*) die "resume takes a chat name from \`ask-claude list\`" ;; esac
    for f in "$STATE/$1"*.json; do
      [ -f "$f" ] || continue
      found="$f"
      n=$((n + 1))
    done
    [ "$n" -le 1 ] || die "\"$1\" matches $n chats, see \`ask-claude list\`"
  else
    # shellcheck disable=SC2012
    found=$(ls -1t "$STATE"/*.json 2>/dev/null | while IFS= read -r f; do
      [ "$(jq length "$f")" -gt 0 ] && {
        echo "$f"
        break
      }
    done)
  fi
  [ -n "$found" ] && [ -f "$found" ] || die "nothing to resume (see \`ask-claude list\`)"
  echo "$found"
}

# chat_loop: one turn per line until /quit; /new starts another chat,
# /history prints this one. On quit, prints the chat and the resume command;
# an empty chat leaves no file.
chat_loop() {
  local line
  echo "ask-claude: $usage_model, history $HIST  (/new resets, /quit exits)" >&2
  while IFS= read -r -e -p "> " line; do
    case "$line" in
    "") continue ;;
    /quit | /exit) break ;;
    /history)
      show_history
      continue
      ;;
    /new)
      [ "$(jq length "$HIST")" -gt 0 ] || rm -f "$HIST" "$HIST.container"
      HIST=$(new_chat)
      echo '[]' >"$HIST"
      echo "new chat: $HIST" >&2
      continue
      ;;
    esac
    ask "$line"
    echo
  done
  if [ "$(jq length "$HIST")" -eq 0 ]; then
    rm -f "$HIST" "$HIST.container"
    return 0
  fi
  echo
  echo "--- history ($HIST) ---" >&2
  show_history
  echo "resume with: ask-claude resume $(basename "$HIST" .json)" >&2
}

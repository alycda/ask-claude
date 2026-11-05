#!/usr/bin/env bash
# claude.sh - talk to Claude with nothing but curl + jq.
#
#   ./claude.sh "one question"          one shot, prints the answer
#   ./claude.sh                          chat loop; /new resets, /quit exits
#   ./claude.sh -f notes.md "summarize"  file contents + prompt, one shot
#   ./claude.sh --resume                 continue the most recent chat
#   ./claude.sh --resume <file|prefix>   continue that chat (name from --list)
#   ./claude.sh --list                   chats on disk, newest first
#   in the loop: /history prints the chat so far, /new starts another,
#   /quit exits and prints the history.
#
# Key: $ANTHROPIC_API_KEY, else ~/.config/anthropic/api_key (mode 600).
#   mise run decrypt personal/anthropic-api-key.age ~/.config/anthropic/api_key
# Knobs (env): MODEL (claude-opus-5-5), EFFORT (low|medium|high|xhigh|max;
#   Opus 5.5 defaults to medium), SYSTEM (system prompt), MAX_TOKENS (16000).
# Skills (env SKILLS): comma-separated "<skill_id>[@<version>]", e.g.
#   SKILLS=skill_014ZAw5Qg64JeAeZbqjPT9UT ./claude.sh   (version defaults to latest)
#   Anthropic's own are SKILLS=pptx,xlsx,docx,pdf. A skill only exists for a
#   request that names it in `container.skills`; the API attaches nothing on
#   its own. Skills run inside Anthropic's code-execution sandbox, so this
#   also adds the code_execution tool and its beta header; the container id
#   is kept per chat ($HIST.container) so files persist across turns.
# History: one JSON file per chat in ~/.local/state/claude-curl/, so a chat
#   can be resumed with HIST=<file> ./claude.sh.
# Caching: the request carries top-level cache_control, so the growing
#   conversation prefix is cached between turns (5 min TTL; a prefix under the
#   model's minimum, 512 tokens on Opus 5.5, silently does not cache). The
#   usage line shows cache_read / cache_write so you can see it working.
# Streaming: every request is SSE. Text prints as it arrives; "(thinking)",
#   "(running code)" and "(result)" markers show what the model is doing while
#   nothing is printing. The event log is reassembled into the full message
#   (thinking blocks, tool use, results) for the history file.
# Bash + curl + jq only. Responses are parsed with jq, never grep.
set -uo pipefail

MODEL="${MODEL:-claude-opus-5-5}"
MAX_TOKENS="${MAX_TOKENS:-16000}"
API="https://api.anthropic.com/v1/messages"
STATE="$HOME/.local/state/claude-curl"
mkdir -p "$STATE"

key="${ANTHROPIC_API_KEY:-}"
if [ -z "$key" ] && [ -r "$HOME/.config/anthropic/api_key" ]; then
    key=$(tr -d '[:space:]' < "$HOME/.config/anthropic/api_key")
fi
[ -n "$key" ] || { echo "claude.sh: no key. export ANTHROPIC_API_KEY or write ~/.config/anthropic/api_key (chmod 600)" >&2; exit 2; }
command -v jq >/dev/null || { echo "claude.sh: jq not found" >&2; exit 2; }

# SAFE: prefix for every jq program that prints text to the terminal. Replies,
# error messages and stored chats (including -f file contents) may carry
# control characters; jq -r decodes \u001b etc. into raw bytes. `safe` keeps
# tab and newline and drops every other C0/C1 control character, so printed
# text cannot set the title, write the clipboard, or erase and forge lines.
SAFE='def safe: if type == "string" then explode | map(select(. == 9 or . == 10 or (. >= 32 and . != 127 and (. < 128 or . > 159)))) | implode else . end; '

# show_history [file]: the chat as "you:" / "claude:" text (thinking blocks skipped)
show_history() {
    jq -r "$SAFE"'.[] | (if .role=="user" then "you" else "claude" end) as $who
        | (if (.content|type)=="string" then .content
           else [.content[] | select(.type=="text") | .text] | join("\n") end) as $t
        | "\($who): \($t)\n" | safe' "${1:-$HIST}"
}

list_chats() { ls -1t "$STATE"/*.json 2>/dev/null | while read -r f; do
        printf '%s  %s turns  %s\n' "$(basename "$f" .json)" "$(jq length "$f")" "$(jq -r "$SAFE"'first(.[]|select(.role=="user"))|.content|if type=="string" then .[:60] else "" end | safe' "$f")"
    done; }

case "${1:-}" in
    --list) list_chats; exit 0 ;;
    --resume)
        if [ -n "${2:-}" ]; then
            if [ -f "$2" ]; then HIST="$2"; else HIST=$(ls -1 "$STATE"/"$2"*.json 2>/dev/null | head -1); fi
        else
            # newest chat that has turns; empty files from aborted starts are skipped
            HIST=$(for f in $(ls -1t "$STATE"/*.json 2>/dev/null); do [ "$(jq length "$f")" -gt 0 ] && { echo "$f"; break; }; done)
        fi
        [ -n "$HIST" ] && [ -f "$HIST" ] || { echo "claude.sh: nothing to resume (see --list)" >&2; exit 2; }
        shift; [ $# -gt 0 ] && shift
        echo "resuming $HIST" >&2; show_history >&2 ;;
esac

HIST="${HIST:-$STATE/$(date +%Y%m%dT%H%M%S).json}"
[ -s "$HIST" ] || echo '[]' > "$HIST"

# skills_json: the container.skills array from $SKILLS ("" when unset).
# Anthropic's four are type "anthropic"; anything starting with skill_ is a
# custom upload (type "custom"). Version is "latest" unless given as @skver_...
skills_json() {
    [ -n "${SKILLS:-}" ] || { echo ""; return; }
    printf '%s' "$SKILLS" | tr ',' '\n' | jq -R -s -c '
        split("\n") | map(select(length>0) | split("@")
        | {skill_id:.[0], version:(.[1] // "latest"),
           type:(if (.[0]|startswith("skill_")) then "custom" else "anthropic" end)})'
}

# call: send the whole history as-is (no new user turn) and append the reply.
# ask <text>: append a user turn, then call. Both print the text blocks and
# return non-zero on API error. A pause_turn (server tool still working) is
# continued automatically, up to 8 rounds.
call() {
    local body resp err stop skills cid betas rounds=0
    skills=$(skills_json)
    betas="server-side-fallback-2026-07-01"
    [ -n "$skills" ] && betas="$betas,code-execution-2025-08-25"
    while :; do
    cid=""; [ -n "$skills" ] && [ -s "$HIST.container" ] && cid=$(cat "$HIST.container")
    body=$(jq -n --arg m "$MODEL" --argjson max "$MAX_TOKENS" --arg sys "${SYSTEM:-}" --arg eff "${EFFORT:-}" \
        --arg skills "$skills" --arg cid "$cid" --slurpfile msgs "$HIST" '
        {model:$m, max_tokens:$max, messages:$msgs[0], fallbacks:"default",
         stream:true, cache_control:{type:"ephemeral"}}
        + (if $sys != "" then {system:$sys} else {} end)
        + (if $eff != "" then {output_config:{effort:$eff}} else {} end)
        + (if $skills != "" then
             {tools:[{type:"code_execution_20260521", name:"code_execution"}],
              container:(({skills:($skills|fromjson)}) + (if $cid != "" then {id:$cid} else {} end))}
           else {} end)')
    local log="$HIST.sse" t0=$SECONDS
    : > "$log"
    # Stream: tee the raw SSE to $log, strip "data: " with builtins (one jq
    # process for the whole stream, not one per token), print text live.
    curl -sS -N --max-time 1800 "$API" \
        -H "content-type: application/json" \
        -H "x-api-key: $key" \
        -H "anthropic-version: 2026-10-21" \
        -H "anthropic-beta: $betas" \
        --data-binary "$body" \
    | tee "$log" \
    | while IFS= read -r l; do case "$l" in "data: "*) printf '%s\n' "${l#data: }" ;; esac; done \
    | jq --unbuffered -rj "$SAFE"'(
        if .type=="content_block_start" then
            (if .content_block.type=="thinking" then "(thinking) "
             elif .content_block.type=="server_tool_use" then "(running code) "
             elif .content_block.type=="bash_code_execution_tool_result" then
                 "(result rc=\(.content_block.content.return_code // "?")) "
             else "" end)
        elif .type=="content_block_delta" and .delta.type=="text_delta" then .delta.text
        elif .type=="message_stop" then "\n"
        else "" end) | safe'
    [ -s "$log" ] || { rm -f "$log"; echo "claude.sh: no response (curl failed or timed out)" >&2; return 1; }
    if ! grep -q '^data: ' "$log"; then
        # not SSE: an HTTP error body
        err=$(jq -r "$SAFE"'.error.message // "unreadable response" | safe' "$log" 2>/dev/null || echo "unreadable response")
    else
        # reassemble the message from the event stream
        resp=$(sed -n 's/^data: //p' "$log" | jq -s '
            reduce .[] as $e ({blocks:{}, model:null, stop_reason:null, stop_details:null, usage:{}, container:null, error:null};
              if $e.type=="message_start" then
                  .model=$e.message.model | .usage=$e.message.usage | .container=$e.message.container
              elif $e.type=="content_block_start" then
                  .blocks[($e.index|tostring)] = ($e.content_block
                      | if .type=="tool_use" or .type=="server_tool_use" then .partial="" else . end)
              elif $e.type=="content_block_delta" then ($e.index|tostring) as $i
                | if $e.delta.type=="text_delta" then .blocks[$i].text += $e.delta.text
                  elif $e.delta.type=="thinking_delta" then .blocks[$i].thinking += $e.delta.thinking
                  elif $e.delta.type=="signature_delta" then .blocks[$i].signature = $e.delta.signature
                  elif $e.delta.type=="input_json_delta" then .blocks[$i].partial += $e.delta.partial_json
                  else . end
              elif $e.type=="content_block_stop" then ($e.index|tostring) as $i
                | if (.blocks[$i] | has("partial")) then
                      .blocks[$i].input = (if .blocks[$i].partial=="" then {} else (.blocks[$i].partial|fromjson) end)
                      | del(.blocks[$i].partial)
                  else . end
              elif $e.type=="message_delta" then
                  .stop_reason=$e.delta.stop_reason | .stop_details=$e.delta.stop_details
                  | .usage=(.usage + ($e.usage // {}))
                  | (if ($e.delta.container? // null) != null then .container=$e.delta.container else . end)
              elif $e.type=="error" then .error=$e.error
              else . end)
            | .content = (.blocks | to_entries | sort_by(.key|tonumber) | map(.value)) | del(.blocks)')
        err=$(printf '%s' "$resp" | jq -r "$SAFE"'.error.message // empty | safe')
    fi
    if [ -n "$err" ]; then
        rm -f "$log"
        echo "claude.sh: API error: $err" >&2
        # drop the user turn so the history stays valid for a retry
        jq '.[:-1]' "$HIST" > "$HIST.tmp" && mv "$HIST.tmp" "$HIST"
        return 1
    fi
    # keep the full content array (thinking blocks included) for the next turn
    printf '%s' "$resp" | jq '{role:"assistant", content:.content}' > "$HIST.turn"
    jq --slurpfile t "$HIST.turn" '. + $t' "$HIST" > "$HIST.tmp" && mv "$HIST.tmp" "$HIST" && rm -f "$HIST.turn"
    printf '%s' "$resp" | jq -r '.container.id // empty' > "$HIST.container.tmp"
    [ -s "$HIST.container.tmp" ] && mv "$HIST.container.tmp" "$HIST.container" || rm -f "$HIST.container.tmp"
    rm -f "$log"
    stop=$(printf '%s' "$resp" | jq -r "$SAFE"'.stop_reason | safe')
    printf '%s' "$resp" | jq -r --argjson s $((SECONDS - t0)) "$SAFE"'"[\(.model) in=\(.usage.input_tokens) cache_read=\(.usage.cache_read_input_tokens // 0) cache_write=\(.usage.cache_creation_input_tokens // 0) out=\(.usage.output_tokens) \($s)s]" | safe' >&2
    if [ "$stop" = "pause_turn" ] && [ $rounds -lt 8 ]; then rounds=$((rounds+1)); echo "[pause_turn: continuing, round $rounds]" >&2; continue; fi
    break
    done
    case "$stop" in
        end_turn) ;;
        refusal)    echo "[stopped: refusal $(printf '%s' "$resp" | jq -r "$SAFE"'.stop_details.category // "" | safe')]" >&2 ;;
        max_tokens) echo "[stopped: max_tokens reached, raise MAX_TOKENS]" >&2 ;;
        *)          echo "[stopped: $stop]" >&2 ;;
    esac
}

ask() {
    jq --arg c "$1" '. + [{"role":"user","content":$c}]' "$HIST" > "$HIST.tmp" && mv "$HIST.tmp" "$HIST"
    call
}

# one shot: optional -f FILE, then the prompt
if [ $# -gt 0 ]; then
    prefix=""
    if [ "$1" = "-f" ]; then
        [ -r "$2" ] || { echo "claude.sh: cannot read $2" >&2; exit 2; }
        prefix="$(cat "$2")"$'\n\n'
        shift 2
    fi
    ask "$prefix$*"
    exit $?
fi

# chat loop
echo "claude.sh: $MODEL, history $HIST  (/new resets, /quit exits)" >&2
while IFS= read -r -e -p "> " line; do
    case "$line" in
        "") continue ;;
        /quit|/exit) break ;;
        /history) show_history; continue ;;
        /new) HIST="$STATE/$(date +%Y%m%dT%H%M%S).json"; echo '[]' > "$HIST"; echo "new chat: $HIST" >&2; continue ;;
    esac
    ask "$line"
    echo
done
if [ "$(jq length "$HIST")" -eq 0 ]; then rm -f "$HIST" "$HIST.container"; exit 0; fi
echo; echo "--- history ($HIST) ---" >&2
show_history
echo "resume with: $0 --resume $(basename "$HIST" .json)" >&2

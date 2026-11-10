#!/usr/bin/env bats
load helpers

STATE_DIR() { echo "$XDG_STATE_HOME/ask-claude"; }
replay() { ASK_CLAUDE_REPLAY="$FIX/sse/$1.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "${@:2}"; }
last_request() { tail -n 1 "$TMP/requests.jsonl"; }

@test "skills: -s adds the tool and typed skills, no container id yet" {
    replay code-execution -s pptx,skill_abc@skver_1 "run" >/dev/null 2>&1
    last_request | jq -e '.tools==[{"type":"code_execution_20260521","name":"code_execution"}]'
    last_request | jq -e '.container.skills==[{"skill_id":"pptx","version":"latest","type":"anthropic"},{"skill_id":"skill_abc","version":"skver_1","type":"custom"}]'
    last_request | jq -e '.container|has("id")|not'
}

@test "skills: the container id from the reply is persisted beside the chat" {
    replay code-execution -s pptx "run" >/dev/null 2>&1
    chat=$(ls "$(STATE_DIR)"/*.json)
    [ "$(cat "${chat%.json}.json.container")" = "container_FIXTURE" ]
}

@test "skills: the persisted id is reused on the next turn of that chat" {
    replay code-execution -s pptx "run" >/dev/null 2>&1
    name=$(basename "$(ls "$(STATE_DIR)"/*.json)" .json)
    bash -c "printf 'again\n/quit\n' | ASK_CLAUDE_REPLAY='$FIX/sse/code-execution.sse' ASK_CLAUDE_REQUEST_LOG='$TMP/requests.jsonl' '$BIN' -s pptx resume $name" >/dev/null 2>&1
    last_request | jq -e '.container.id=="container_FIXTURE"'
}

@test "skills: without -s there is no tool and no container" {
    replay text "hi" >/dev/null 2>&1
    last_request | jq -e '(has("tools") or has("container"))|not'
    [ -z "$(ls "$(STATE_DIR)"/*.container 2>/dev/null)" ]
}

@test "skills: the stream shows running and result markers" {
    run --separate-stderr replay code-execution -s pptx "run"
    grep -q '(running code) (result rc=0) 1024' <<<"$output"
}

# web_container_stream: text.sse whose reply carries container_WEB. Fails
# when nothing was injected, so a test asserting no file is left cannot pass
# without a container to leave one.
web_container_stream() {
    sed 's/"container":null/"container":{"id":"container_WEB","expires_at":"2026-11-10T01:00:00Z"}/g' "$FIX/sse/text.sse" >"$TMP/web-container.sse"
    grep -q '"container":{"id":"container_WEB"' "$TMP/web-container.sse" || { echo "web_container_stream: no container injected into text.sse"; return 1; }
}

# A --web reply carries a container too (dynamic filtering). The chat's
# container file is the skills container: a turn without -s must neither
# overwrite nor delete it, or the next -s turn loses its files (0.3.1).
@test "skills: a later turn without -s keeps the chat's skills container" {
    replay code-execution -s pptx "run" >/dev/null 2>&1
    chat=$(ls "$(STATE_DIR)"/*.json)
    name=$(basename "$chat" .json)
    web_container_stream
    bash -c "printf 'again\n/quit\n' | ASK_CLAUDE_REPLAY='$TMP/web-container.sse' '$BIN' --web resume $name" >/dev/null 2>&1
    [ "$(cat "${chat%.json}.json.container")" = "container_FIXTURE" ]
    bash -c "printf 'once more\n/quit\n' | ASK_CLAUDE_REPLAY='$FIX/sse/code-execution.sse' ASK_CLAUDE_REQUEST_LOG='$TMP/requests.jsonl' '$BIN' -s pptx resume $name" >/dev/null 2>&1
    last_request | jq -e '.container.id=="container_FIXTURE"'
}

@test "skills: without -s a container in the reply leaves no file beside the chat" {
    web_container_stream
    ASK_CLAUDE_REPLAY="$TMP/web-container.sse" "$BIN" --web "hi" >/dev/null 2>&1
    [ -z "$(ls "$(STATE_DIR)"/*.container 2>/dev/null)" ]
}

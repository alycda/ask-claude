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

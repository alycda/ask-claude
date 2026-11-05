#!/usr/bin/env bats
# tests/record-session.sh --redact: what reaches a tracked fixture. No
# network: the redaction runs on a canned event line.

load helpers

REC="$ROOT/tests/record-session.sh"
LINE='data: {"id":"sevt_abc","session_id":"sesn_X1","agent_name":"My Private Agent","agent_id":"agent_Q","file_id":"file_Z9","ids":["sevt_abc","sevt_def"],"store":"memstore_M1","env":"env_E","vault":"vlt_V","thread":"sthr_T"}'

@test "record-session: ids become stable FIXTURE ids, keys stay keys" {
    run bash "$REC" --redact <<<"$LINE"
    [ "$status" -eq 0 ]
    jq -e '
        .id == "sevt_FIXTURE01" and .ids == ["sevt_FIXTURE01", "sevt_FIXTURE02"]
        and .session_id == "sesn_FIXTURE01" and .agent_id == "agent_FIXTURE01"
        and .file_id == "file_FIXTURE01" and .store == "memstore_FIXTURE01"
        and .env == "env_FIXTURE01" and .vault == "vlt_FIXTURE01" and .thread == "sthr_FIXTURE01"' <<<"${output#data: }"
}

@test "record-session: the agent's name never reaches the fixture" {
    run bash "$REC" --redact <<<"$LINE"
    [ "$status" -eq 0 ]
    jq -e '.agent_name == "fixture"' <<<"${output#data: }"
    if grep -q 'Private' <<<"$output"; then return 1; fi
}

@test "record-session: ASK_CLAUDE_IDMAP keeps an id's FIXTURE number across streams" {
    local s1='data: {"type":"user.message","id":"sevt_u1"}
data: {"type":"agent.tool_use","id":"sevt_tool","session_id":"sesn_real"}'
    local s2='data: {"type":"user.tool_confirmation","id":"sevt_conf","tool_use_id":"sevt_tool"}
data: {"type":"agent.tool_result","id":"sevt_res","tool_use_id":"sevt_tool","session_id":"sesn_real"}'
    ASK_CLAUDE_IDMAP="$TMP/idmap" bash "$REC" --redact <<<"$s1" >"$TMP/1.sse"
    ASK_CLAUDE_IDMAP="$TMP/idmap" bash "$REC" --redact <<<"$s2" >"$TMP/2.sse"
    # the tool_use is sevt_FIXTURE02 in stream 1 and still is in stream 2;
    # new ids in stream 2 continue the numbering instead of restarting it
    jq -e -s '.[1].id == "sevt_FIXTURE02"' < <(sed 's/^data: //' "$TMP/1.sse")
    jq -e -s '.[0].tool_use_id == "sevt_FIXTURE02" and .[0].id == "sevt_FIXTURE03"
              and .[1].id == "sevt_FIXTURE04" and .[1].session_id == "sesn_FIXTURE01"' < <(sed 's/^data: //' "$TMP/2.sse")
    # the map holds real ids, so only its owner may read it
    [ "$(stat -c %a "$TMP/idmap" 2>/dev/null || stat -f %Lp "$TMP/idmap")" = 600 ]
}

@test "record-session: without ASK_CLAUDE_IDMAP each stream numbers from 01" {
    local s2='data: {"type":"agent.tool_result","id":"sevt_res","tool_use_id":"sevt_tool"}'
    run bash "$REC" --redact <<<"$s2"
    jq -e '.id == "sevt_FIXTURE01" and .tool_use_id == "sevt_FIXTURE02"' <<<"${output#data: }"
}

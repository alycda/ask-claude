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

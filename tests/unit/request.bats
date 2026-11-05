#!/usr/bin/env bats
load helpers

# lib/request.jq: `jq -n` with claude.sh's argument names -> request body.
# lib/skills.jq: `jq -R -s -c` over "<id>[@<ver>]\n..." -> container.skills.

request() { # sys eff skills cid
    jq -n --arg m claude-opus-5-5 --argjson max 16000 \
        --arg sys "$1" --arg eff "$2" --arg skills "$3" --arg cid "$4" \
        --slurpfile msgs "$FIX/request/history-one-turn.json" \
        -f "$ROOT/lib/request.jq"
}
SKILLS='[{"skill_id":"pptx","version":"latest","type":"anthropic"},{"skill_id":"skill_abc","version":"skver_1","type":"custom"}]'

@test "request: plain" {
    request "" "" "" "" > "$TMP/out.json"
    assert_golden "$FIX/request/plain.json" "$TMP/out.json"
    jq -e '.stream==true and .fallbacks=="default" and .cache_control.type=="ephemeral"' "$TMP/out.json"
    jq -e 'has("system")|not' "$TMP/out.json"
    jq -e 'has("output_config")|not' "$TMP/out.json"
}

@test "request: system prompt and effort" {
    request "be brief" low "" "" > "$TMP/out.json"
    assert_golden "$FIX/request/system-effort.json" "$TMP/out.json"
    jq -e '.system=="be brief" and .output_config.effort=="low"' "$TMP/out.json"
}

@test "request: skills add the code_execution tool and a container" {
    request "" "" "$SKILLS" "" > "$TMP/out.json"
    assert_golden "$FIX/request/skills-request.json" "$TMP/out.json"
    jq -e '.tools[0].type=="code_execution_20260521"' "$TMP/out.json"
    jq -e '.container|has("id")|not' "$TMP/out.json"
}

@test "request: a known container id is reused" {
    request "" "" "$SKILLS" container_FIXTURE > "$TMP/out.json"
    assert_golden "$FIX/request/skills-container.json" "$TMP/out.json"
    jq -e '.container.id=="container_FIXTURE"' "$TMP/out.json"
}

@test "skills: comma list becomes typed entries, version defaults to latest" {
    printf 'pptx,skill_abc@skver_1' | tr ',' '\n' | jq -R -s -c -f "$ROOT/lib/skills.jq" > "$TMP/out.json"
    assert_golden "$FIX/request/skills.json" "$TMP/out.json"
}

@test "skills: empty input gives an empty array" {
    printf '' | jq -R -s -c -f "$ROOT/lib/skills.jq" | grep -qx '\[\]'
}

# web search, web fetch and MCP servers (0.4.0)

request2() { # sys eff skills cid web mcp
    jq -n --arg m claude-opus-5-5 --argjson max 16000 \
        --arg sys "$1" --arg eff "$2" --arg skills "$3" --arg cid "$4" --arg web "$5" --arg mcp "$6" \
        --slurpfile msgs "$FIX/request/history-one-turn.json" \
        -f "$ROOT/lib/request.jq"
}

@test "request: --web adds the basic web search and web fetch server tools" {
    request2 "" "" "" "" 1 "" > "$TMP/out.json"
    assert_golden "$FIX/request/web.json" "$TMP/out.json"
}

@test "request: --mcp adds a server per URL, named after its host, and one toolset each" {
    request2 "" "" "" "" "" "https://mcp.deepwiki.com/mcp https://mcp.example.com/sse" > "$TMP/out.json"
    assert_golden "$FIX/request/mcp.json" "$TMP/out.json"
    jq -e '.mcp_servers[0] | has("authorization_token") | not' "$TMP/out.json"
}

@test "request: skills, web and an MCP server with a token from the environment, in one body" {
    ASK_CLAUDE_MCP_TOKEN=tok_FIXTURE request2 "" "" '[{"skill_id":"pptx","version":"latest","type":"anthropic"}]' "" 1 "https://mcp.deepwiki.com/mcp" > "$TMP/out.json"
    assert_golden "$FIX/request/everything.json" "$TMP/out.json"
}

@test "request: without --web or --mcp the old goldens are untouched" {
    request2 "" "" "" "" "" "" > "$TMP/out.json"
    assert_golden "$FIX/request/plain.json" "$TMP/out.json"
}

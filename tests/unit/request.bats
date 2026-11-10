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

@test "request: --web adds the web search and web fetch server tools, with dynamic filtering" {
    request2 "" "" "" "" 1 "" > "$TMP/out.json"
    assert_golden "$FIX/request/web.json" "$TMP/out.json"
    jq -e '[.tools[].type] == ["web_search_20260318", "web_fetch_20260318"]' "$TMP/out.json"
}

# dynamic filtering needs Claude 4.6 or later: an older model id gets the
# basic versions, which every model takes (0.3.1)
web_types() { # model
    jq -n -c --arg m "$1" --argjson max 16000 --arg sys "" --arg eff "" --arg skills "" --arg cid "" --arg web 1 \
        --slurpfile msgs "$FIX/request/history-one-turn.json" -f "$ROOT/lib/request.jq" | jq -c '[.tools[].type]'
}

@test "request: --web on a model older than 4.6 sends the basic versions" {
    jq -n --arg m claude-haiku-4-5 --argjson max 16000 --arg sys "" --arg eff "" --arg skills "" --arg cid "" --arg web 1 --arg mcp "" \
        --slurpfile msgs "$FIX/request/history-one-turn.json" -f "$ROOT/lib/request.jq" > "$TMP/out.json"
    assert_golden "$FIX/request/web-basic.json" "$TMP/out.json"
    local m
    for m in claude-haiku-4-5 claude-haiku-4-5-20251001 claude-sonnet-4-5-20250929 claude-opus-4-1 claude-opus-4-20250514 claude-sonnet-4-0 claude-3-7-sonnet-20250219; do
        [ "$(web_types "$m")" = '["web_search_20250305","web_fetch_20250910"]' ] || { echo "basic expected for $m"; return 1; }
    done
}

@test "request: --web on 4.6 and later, and on every alias, sends the dynamic versions" {
    local m
    for m in claude-opus-5-5 claude-sonnet-5-5 claude-haiku-5-5 claude-fable-5-1 claude-opus-4-6 claude-sonnet-4-6 claude-opus-4-8 haiku sonnet opus fable; do
        [ "$(web_types "$m")" = '["web_search_20260318","web_fetch_20260318"]' ] || { echo "dynamic expected for $m"; return 1; }
    done
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

# One ASK_CLAUDE_MCP_TOKEN is one server's credential. Sent to every server,
# the operator of each would receive the token issued for the others
# (docs/security-audit/NEEDS-VALIDATION.md, mcp_servers:shared-authorization-
# token). The shared variable reaches a lone server only; each server of
# several takes ASK_CLAUDE_MCP_TOKEN_<HOST>, the host upper-cased with every
# other character an underscore.

@test "request: with several servers the shared token reaches none of them" {
    ASK_CLAUDE_MCP_TOKEN=tok_SHARED request2 "" "" "" "" "" "https://mcp.deepwiki.com/mcp https://mcp.example.com/sse" > "$TMP/out.json"
    jq -e 'all(.mcp_servers[]; has("authorization_token") | not)' "$TMP/out.json"
}

@test "request: each server takes its own ASK_CLAUDE_MCP_TOKEN_<HOST>, and only its own" {
    ASK_CLAUDE_MCP_TOKEN_MCP_DEEPWIKI_COM=tok_A ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM=tok_B \
        request2 "" "" "" "" "" "https://mcp.deepwiki.com/mcp https://mcp.example.com/sse" > "$TMP/out.json"
    jq -e '.mcp_servers | map(.authorization_token) == ["tok_A", "tok_B"]' "$TMP/out.json"
    ASK_CLAUDE_MCP_TOKEN_MCP_DEEPWIKI_COM=tok_A \
        request2 "" "" "" "" "" "https://mcp.deepwiki.com/mcp https://mcp.example.com/sse" > "$TMP/out.json"
    jq -e '.mcp_servers[0].authorization_token == "tok_A" and (.mcp_servers[1] | has("authorization_token") | not)' "$TMP/out.json"
}

@test "request: a lone server's own token wins over the shared one" {
    ASK_CLAUDE_MCP_TOKEN=tok_SHARED ASK_CLAUDE_MCP_TOKEN_MCP_DEEPWIKI_COM=tok_A \
        request2 "" "" "" "" "" "https://mcp.deepwiki.com/mcp" > "$TMP/out.json"
    jq -e '.mcp_servers[0].authorization_token == "tok_A"' "$TMP/out.json"
}

@test "request: without --web or --mcp the old goldens are untouched" {
    request2 "" "" "" "" "" "" > "$TMP/out.json"
    assert_golden "$FIX/request/plain.json" "$TMP/out.json"
}

# the advisor tool (0.2.6)

@test "request: --advisor adds the advisor tool, last, capped at 3 calls a turn" {
    jq -n --arg m claude-haiku-5-5 --argjson max 16000 \
        --arg sys "" --arg eff "" --arg skills "" --arg cid "" --arg web 1 --arg mcp "" --arg advisor claude-sonnet-5 \
        --slurpfile msgs "$FIX/request/history-one-turn.json" \
        -f "$ROOT/lib/request.jq" > "$TMP/out.json"
    assert_golden "$FIX/request/advisor.json" "$TMP/out.json"
}

@test "request: without --advisor the web golden is untouched" {
    request2 "" "" "" "" 1 "" > "$TMP/out.json"
    assert_golden "$FIX/request/web.json" "$TMP/out.json"
}

# the cap is 3 a turn: a pause_turn continuation is a new request, so the
# advisor calls already in this turn's paused replies count against it

# advisor_history <calls this turn> <calls in the turn before>: a chat whose
# last user message is followed by one paused reply holding that many calls
advisor_history() {
    jq -n --argjson now "$1" --argjson before "$2" '
        def call: {type: "server_tool_use", id: "srvtoolu_FIXTURE", name: "advisor", input: {}};
        def reply(n): {role: "assistant", content: ([range(n) | call] + [{type: "text", text: "..."}])};
        [{role: "user", content: "first"}, reply($before),
         {role: "user", content: "second"}, reply($now)]' > "$TMP/hist.json"
}
advisor_request() {
    jq -n --arg m claude-haiku-5-5 --argjson max 16000 \
        --arg sys "" --arg eff "" --arg skills "" --arg cid "" --arg web "" --arg mcp "" --arg advisor claude-sonnet-5 \
        --slurpfile msgs "$TMP/hist.json" -f "$ROOT/lib/request.jq"
}

@test "request: a paused turn's advisor calls count against the 3" {
    advisor_history 2 0
    advisor_request | jq -e '.tools == [{type: "advisor_20260301", name: "advisor", model: "claude-sonnet-5", max_uses: 1}]'
}

@test "request: with 3 calls made this turn the advisor tool is left out" {
    advisor_history 3 0
    advisor_request | jq -e 'has("tools") | not'
}

@test "request: calls from an earlier turn do not count" {
    advisor_history 0 3
    advisor_request | jq -e '.tools[0].max_uses == 3'
}

# --mcp-tool: a server named by one or more entries offers only those tools
# (0.3.1). The toolset becomes an allowlist: every tool off, the named ones on.

request_mcptools() { # mcp mcptools
    jq -n --arg m claude-opus-5-5 --argjson max 16000 \
        --arg sys "" --arg eff "" --arg skills "" --arg cid "" --arg web "" --arg mcp "$1" --arg advisor "" \
        --arg mcptools "$2" \
        --slurpfile msgs "$FIX/request/history-one-turn.json" -f "$ROOT/lib/request.jq"
}

@test "request: --mcp-tool makes that server's toolset an allowlist; a server it does not name keeps every tool" {
    request_mcptools "https://mcp.deepwiki.com/mcp https://mcp.example.com/sse" \
        "mcp.deepwiki.com:read_wiki_structure mcp.deepwiki.com:ask_question" > "$TMP/out.json"
    assert_golden "$FIX/request/mcp-tool.json" "$TMP/out.json"
    jq -e '.tools[0].default_config == {enabled: false}
        and .tools[0].configs == {read_wiki_structure: {enabled: true}, ask_question: {enabled: true}}' "$TMP/out.json"
    jq -e '.tools[1] == {type: "mcp_toolset", mcp_server_name: "mcp-example-com"}' "$TMP/out.json"
}

@test "request: --mcp-tool matches the host whatever its case, and ignores the URL's port and path" {
    request_mcptools "https://MCP.Example.com:8443/sse" "mcp.EXAMPLE.com:search" |
        jq -e '.tools[0].configs == {search: {enabled: true}}'
}

@test "request: without --mcp-tool the MCP golden is untouched" {
    request_mcptools "https://mcp.deepwiki.com/mcp https://mcp.example.com/sse" "" > "$TMP/out.json"
    assert_golden "$FIX/request/mcp.json" "$TMP/out.json"
}

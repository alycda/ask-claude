# Contract test for the one request ask-claude makes.
#
#   hurl --test \
#     --variable base_url=https://api.anthropic.com \
#     --secret   api_key="$ANTHROPIC_API_KEY" \
#     tests/messages.hurl
#
# Swap base_url for a local stub to run without spending a call.

POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
Content-Type: application/json
{
  "model": "claude-haiku-5-5",
  "max_tokens": 64,
  "output_config": { "effort": "low" },
  "messages": [
    { "role": "user", "content": "Reply with the single word: pong" }
  ]
}
HTTP 200
[Asserts]
jsonpath "$.type" == "message"
jsonpath "$.stop_reason" == "end_turn"
jsonpath "$.content[0].type" == "text"
jsonpath "$.content[0].text" contains "pong"
jsonpath "$.usage.output_tokens" > 0

# --mcp-tool: an allowlist toolset (every tool off, the named one on) is
# accepted, and only the named tool reaches the model. Listing the tools
# makes no MCP call. The allowed name must be listed: a name the server
# doesn't have is accepted too, and leaves the model with no tools at all
# (0.3.1). Depends on mcp.deepwiki.com being up.
POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: mcp-client-2025-11-20
Content-Type: application/json
{
  "model": "claude-haiku-5-5",
  "max_tokens": 128,
  "output_config": { "effort": "low" },
  "mcp_servers": [
    { "type": "url", "url": "https://mcp.deepwiki.com/mcp", "name": "mcp-deepwiki-com" }
  ],
  "tools": [
    {
      "type": "mcp_toolset",
      "mcp_server_name": "mcp-deepwiki-com",
      "default_config": { "enabled": false },
      "configs": { "ask_wiki_question": { "enabled": true } }
    }
  ],
  "messages": [
    { "role": "user", "content": "List the exact names of every tool you can call, one per line, nothing else. Do not call any tool." }
  ]
}
HTTP 200
[Asserts]
jsonpath "$.stop_reason" == "end_turn"
body contains "ask_wiki_question"
body not contains "read_wiki_structure"
body not contains "read_wiki_contents"

# --web on 4.6+: the dynamic-filtering web search is accepted with no beta
# header, runs code, and returns the container that code ran in (0.3.1).
POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
Content-Type: application/json
{
  "model": "claude-haiku-5-5",
  "max_tokens": 1024,
  "output_config": { "effort": "low" },
  "tools": [
    { "type": "web_search_20260318", "name": "web_search", "max_uses": 1 }
  ],
  "messages": [
    { "role": "user", "content": "Search the web once for: jq latest release. Reply in five words." }
  ]
}
HTTP 200
[Captures]
container_id: jsonpath "$.container.id"
[Asserts]
jsonpath "$.content[?(@.type=='server_tool_use')]" count > 0
jsonpath "$.content[?(@.type=='code_execution_tool_result')]" count > 0
jsonpath "$.container.id" startsWith "container_"

# A pause_turn continuation without -s sends the container back as a plain
# string, with no code-execution beta. If this fails, every paused --web
# turn without -s fails on its follow-up (0.3.1). This proves the form is
# accepted, not that the container is reused: on 2026-11-10 the API took
# any string here (even "totally-bogus"), returned 200 and echoed it back.
POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
Content-Type: application/json
{
  "model": "claude-haiku-5-5",
  "max_tokens": 1024,
  "output_config": { "effort": "low" },
  "container": "{{container_id}}",
  "tools": [
    { "type": "web_search_20260318", "name": "web_search", "max_uses": 1 }
  ],
  "messages": [
    { "role": "user", "content": "Search the web once for: jq latest release. Reply in five words." }
  ]
}
HTTP 200
[Asserts]
jsonpath "$.stop_reason" == "end_turn"
jsonpath "$.content[?(@.type=='code_execution_tool_result')]" count > 0

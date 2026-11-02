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

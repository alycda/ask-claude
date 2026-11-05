# Live contract for one session turn: create with a prompt and a 10-cent
# budget, wait for idle, read the events, archive. Needs an agent and an
# environment made in the Console (ASK_CLAUDE_AGENT, ASK_CLAUDE_ENVIRONMENT).
#
#   just test-agents
#
POST {{base_url}}/v1/sessions
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: managed-agents-2026-04-01
Content-Type: application/json
{
  "agent": { "type": "agent", "id": "{{agent}}" },
  "environment_id": "{{environment}}",
  "title": "ask-claude contract",
  "budget": { "type": "limit", "max_list_cost": { "amount": "10", "currency": "USD" } },
  "initial_events": [
    { "type": "user.message", "content": [{ "type": "text", "text": "Reply with the single word: pong" }] }
  ]
}
HTTP 200
[Captures]
sesn: jsonpath "$.id"
[Asserts]
jsonpath "$.status" == "running"
jsonpath "$.budget.max_list_cost.amount" == "10"

# the sandbox takes a while to start; poll until the turn ends
GET {{base_url}}/v1/sessions/{{sesn}}
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: managed-agents-2026-04-01
[Options]
retry: 40
retry-interval: 3000
HTTP 200
[Asserts]
jsonpath "$.status" == "idle"
jsonpath "$.usage.output_tokens" > 0
jsonpath "$.usage.list_cost.amount" exists

GET {{base_url}}/v1/sessions/{{sesn}}/events
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: managed-agents-2026-04-01
HTTP 200
[Asserts]
jsonpath "$.data[*].type" contains "user.message"
jsonpath "$.data[*].type" contains "agent.message"
jsonpath "$.data[*].type" contains "session.usage"
jsonpath "$.data[*].type" contains "session.status_idle"
jsonpath "$.data[?(@.type=='agent.message')].content[0].text" contains "pong"

POST {{base_url}}/v1/sessions/{{sesn}}/archive
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: managed-agents-2026-04-01
HTTP 200
[Asserts]
jsonpath "$.archived_at" exists
jsonpath "$.status" == "terminated"

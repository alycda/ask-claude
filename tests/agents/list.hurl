# Live contract for the lists ask-claude reads: agents and environments
# (just agents, just environments) and sessions (ask-claude agent list).
#
#   just test-agents
#
GET {{base_url}}/v1/agents
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: managed-agents-2026-04-01
HTTP 200
[Asserts]
jsonpath "$.data" isCollection
jsonpath "$.data[?(@.id=='{{agent}}')].id" == "{{agent}}"

GET {{base_url}}/v1/environments
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: managed-agents-2026-04-01
HTTP 200
[Asserts]
jsonpath "$.data[?(@.id=='{{environment}}')].id" == "{{environment}}"

GET {{base_url}}/v1/sessions?order=desc&limit=5
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: managed-agents-2026-04-01
HTTP 200
[Asserts]
jsonpath "$.data" isCollection
jsonpath "$.data[0].usage.list_cost.amount" exists

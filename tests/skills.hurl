# Live contract for -s: the code-execution tool, its beta, container.skills,
# and the container id in the reply that ask-claude keeps beside the chat
# ($HIST.container) and sends back next turn. It starts a sandbox, so it is
# its own recipe, `just test-skills`, not part of `just test`.
#
# Adapted from the claude-rs prototype's tests/hurl/skills.hurl.

POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: server-side-fallback-2026-07-01,code-execution-2025-08-25
Content-Type: application/json
{
  "model": "claude-haiku-5-5",
  "max_tokens": 1024,
  "fallbacks": "default",
  "tools": [{"type": "code_execution_20260521", "name": "code_execution"}],
  "container": {"skills": [{"type": "anthropic", "skill_id": "pptx", "version": "latest"}]},
  "messages": [{"role": "user", "content": "Run `echo hello` with the code execution tool and reply with its output."}]
}
HTTP 200
[Asserts]
jsonpath "$.container.id" isString
jsonpath "$.content[*].type" contains "server_tool_use"
jsonpath "$.content[*].type" contains "bash_code_execution_tool_result"

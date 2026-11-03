# Live contract for the streaming request ask-claude makes.
#
#   just test        (loads ANTHROPIC_API_KEY from .env)
#
POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
anthropic-beta: server-side-fallback-2026-07-01
Content-Type: application/json
{
  "model": "claude-opus-5-5",
  "max_tokens": 32,
  "stream": true,
  "fallbacks": "default",
  "cache_control": { "type": "ephemeral" },
  "output_config": { "effort": "low" },
  "messages": [
    { "role": "user", "content": "Reply with the single word: pong" }
  ]
}
HTTP 200
[Asserts]
header "content-type" contains "text/event-stream"
body contains "event: message_start"
body contains "\"type\":\"content_block_delta\""
body contains "event: message_delta"
body contains "event: message_stop"

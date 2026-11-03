# Streaming chat completions, OpenAI shape. One file for every provider that
# speaks it; the provider is three variables:
#
#   base_url   https://api.openai.com/v1      https://api.venice.ai/api/v1
#   api_key    OPENAI_API_KEY                 VENICE_API_KEY
#   model      gpt-4.1-mini                   zai-org-glm-5-1
#
#   just test-providers
#
POST {{base_url}}/chat/completions
Authorization: Bearer {{api_key}}
Content-Type: application/json
{
  "model": "{{model}}",
  "stream": true,
  "max_completion_tokens": 16,
  "stream_options": { "include_usage": true },
  "messages": [
    { "role": "user", "content": "Reply with the single word: pong" }
  ]
}
HTTP 200
[Asserts]
header "content-type" contains "text/event-stream"
body contains "\"object\":\"chat.completion.chunk\""
body contains "\"finish_reason\":\"stop\""
body contains "data: [DONE]"

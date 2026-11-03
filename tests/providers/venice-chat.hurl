# Venice chat completions, streaming (OpenAI-compatible). Needs VENICE_API_KEY in .env.
#   just test-providers
POST https://api.venice.ai/api/v1/chat/completions
Authorization: Bearer {{venice_key}}
Content-Type: application/json
[Options]
variable: venice_model=zai-org-glm-5-1
{
  "model": "{{venice_model}}",
  "stream": true,
  "max_tokens": 16,
  "messages": [
    { "role": "user", "content": "Reply with the single word: pong" }
  ]
}
HTTP 200
[Asserts]
header "content-type" contains "text/event-stream"
body contains "\"object\":\"chat.completion.chunk\""
body contains "data: [DONE]"

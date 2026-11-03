# OpenAI chat completions, streaming. Needs OPENAI_API_KEY in .env.
#   just test-providers
POST https://api.openai.com/v1/chat/completions
Authorization: Bearer {{openai_key}}
Content-Type: application/json
[Options]
variable: openai_model=gpt-4.1-mini
{
  "model": "{{openai_model}}",
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

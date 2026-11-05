# Streaming chat completions, OpenAI shape. One file for every provider that
# speaks it; the provider is three variables, which `just test-openai` takes
# from .env the way ask-claude's -p openai --base-url does:
#
#   base_url   OPENAI_BASE_URL   https://api.openai.com/v1   https://api.venice.ai/api/v1
#   api_key    OPENAI_API_KEY    an OpenAI key               a Venice key
#   model      OPENAI_MODEL      gpt-4.1-mini                gemini-4-argon (the default)
#
#   just test-openai
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

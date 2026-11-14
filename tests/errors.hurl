# Live contract for the error body ask-claude reads into "API error: <msg>"
# (bash `api_error`, the Rust `api_error`). tests/fixtures/sse/error.sse is
# hand-derived; this is what keeps it honest. None of these is billed: each
# fails before the model runs.
#
#   just test        (loads ANTHROPIC_API_KEY from .env)
#
# Adapted from the claude-rs prototype's tests/hurl/errors.hurl.

# A bad key: 401, the same envelope.
POST {{base_url}}/v1/messages
x-api-key: invalid-key-on-purpose
anthropic-version: 2026-10-21
Content-Type: application/json
{"model": "claude-opus-5-5", "max_tokens": 16, "messages": [{"role": "user", "content": "hi"}]}
HTTP 401
[Asserts]
header "Content-Type" contains "application/json"
jsonpath "$.type" == "error"
jsonpath "$.error.type" == "authentication_error"
jsonpath "$.error.message" isString

# An unknown model: 404 with exactly the body error.sse holds.
POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
Content-Type: application/json
{"model": "claude-does-not-exist", "max_tokens": 16, "messages": [{"role": "user", "content": "hi"}]}
HTTP 404
[Asserts]
header "Content-Type" contains "application/json"
jsonpath "$.type" == "error"
jsonpath "$.error.type" == "not_found_error"
jsonpath "$.error.message" == "model: claude-does-not-exist"
jsonpath "$.request_id" isString

# A missing required field: 400 invalid_request_error, the field named.
POST {{base_url}}/v1/messages
x-api-key: {{api_key}}
anthropic-version: 2026-10-21
Content-Type: application/json
{"model": "claude-opus-5-5", "messages": [{"role": "user", "content": "hi"}]}
HTTP 400
[Asserts]
jsonpath "$.type" == "error"
jsonpath "$.error.type" == "invalid_request_error"
jsonpath "$.error.message" contains "max_tokens"

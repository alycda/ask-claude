set dotenv-load
set dotenv-required

help:
    ./ask-claude --help

[working-directory: 'tests']
test:
    hurl --test --variable base_url=https://api.anthropic.com --secret api_key="$ANTHROPIC_API_KEY" .

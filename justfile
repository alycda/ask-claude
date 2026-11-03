set dotenv-load
set dotenv-required

help:
    ./ask-claude --help

# offline: bats over recorded fixtures (tests/unit), no key needed
unit-tests:
    bats tests/unit

unit-test unit:
    bats tests/unit/{{unit}}.bats

# live Anthropic contracts; one cheap call each
[working-directory: 'tests']
test:
    hurl --test --variable base_url=https://api.anthropic.com --secret api_key="$ANTHROPIC_API_KEY" .

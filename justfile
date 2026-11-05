help:
    ./ask-claude --help

# offline: bats over recorded fixtures (tests/unit), no key needed
unit-tests:
    bats tests/unit

unit-test unit:
    bats tests/unit/{{ quote(unit) }}.bats

step name:
    bats --filter-tags {{ quote("step:" + name) }} tests/unit/*-steps.bats

# live Anthropic contracts; one cheap call each. Only this recipe reads .env, and
# the key reaches hurl through a secrets file on a pipe, not on its command line.
[working-directory: 'tests']
test:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; . "{{ justfile_directory() }}/.env"; set +a
    hurl --test --variable base_url=https://api.anthropic.com \
        --secrets-file <(printf 'api_key=%s\n' "$ANTHROPIC_API_KEY") .

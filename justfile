import 'tools/effver/effver.just'

help:
    ./ask-claude --help

ask question="" *FLAGS:
    ./ask-claude {{FLAGS}} {{ quote(question) }}

# shell completions from the #USAGE spec (bash, fish, nu, powershell, zsh). The
# script embeds the spec, so re-run it after the spec changes. --install writes
# it where the shell looks (zsh ~/.local/share/zsh/site-functions/_ask-claude,
# bash ~/.local/share/bash-completion/completions/ask-claude,
# fish ~/.config/fish/completions/ask-claude.fish) and prints the one-time
# rc line zsh needs; without it the script goes to stdout.

# tab completion for <shell> from the #USAGE spec; --install puts it where the shell looks
completions shell *flags:
    usage generate completion {{ quote(shell) }} ask-claude -f ask-claude {{ flags }}

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
        --secrets-file <(printf 'api_key=%s\n' "$ANTHROPIC_API_KEY") messages.hurl anthropic-stream.hurl

# Reads .env like test; the key reaches curl through a header file on a pipe.

# custom skills on this account (id, title, latest version), for -s
skills:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; . "{{ justfile_directory() }}/.env"; set +a
    curl -sS 'https://api.anthropic.com/v1/skills?source=custom&limit=100' \
        -H @<(printf 'x-api-key: %s\nanthropic-version: 2026-10-21\n' "$ANTHROPIC_API_KEY") |
        jq -r 'if .data then .data[] | [.id, (.display_title // "-"), (.latest_version // "-")] | @tsv
               else "just skills: \(.error.message // "unreadable response")\n" | halt_error end'

# live OpenAI-style contracts; needs OPENAI_API_KEY and VENICE_API_KEY in .env
[working-directory: 'tests/providers']
test-providers:
    hurl --test --secret openai_key="$OPENAI_API_KEY" --secret venice_key="$VENICE_API_KEY" openai-chat.hurl venice-chat.hurl

import 'tools/effver/effver.just'

help:
    ./ask-claude --help

agents-help:
    ./ask-claude agent --help

[group('rust')]
_help: build
    rust/target/debug/ask-claude --help

[group('rust')]
_agent-help: build
    rust/target/debug/ask-claude agent --help

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

# Skips the tests tagged pty, which need a terminal name readline reports
# bracketed paste for (`just pty-test` runs them), and rust-only, behaviour the
# script does not have.

# offline: bats over recorded fixtures (tests/unit), no key needed
unit-tests:
    bats --filter-tags '!pty,!rust-only' tests/unit

# the pty-tagged tests alone, under a TERM readline reports bracketed paste for
pty-test:
    TERM=xterm-256color bats --filter-tags pty tests/unit

# the GitHub Actions workflows; mise fetches actionlint, so mise.toml stays the script's own toolchain
actionlint:
    mise x actionlint@1.7.12 -- actionlint

unit-test unit:
    bats tests/unit/{{ quote(unit) }}.bats

step name:
    bats --filter-tags {{ quote("step:" + name) }} tests/unit/*-steps.bats

# live Anthropic contracts: cheap calls, two of them one web search each (1 cent
# per search), one listing an MCP server's tools. Only the live recipes (this one and
# test-openai) read .env, and the key reaches hurl through a secrets file on a
# pipe, not on its command line.
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

# live Managed Agents contracts: one session turn (10 cents, archived after) and
# the three lists. Reads .env like test, plus ASK_CLAUDE_AGENT and
# ASK_CLAUDE_ENVIRONMENT, the ids from the Console. Never run by `test`.

# live Managed Agents contracts: the three lists and one 10-cent session turn, archived after
[working-directory: 'tests/agents']
test-agents:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; . "{{ justfile_directory() }}/.env"; set +a
    : "${ASK_CLAUDE_AGENT:?set it in .env, the agent id from the Console}" "${ASK_CLAUDE_ENVIRONMENT:?set it in .env, the environment id from the Console}"
    hurl --test --variable base_url=https://api.anthropic.com \
        --variable agent="$ASK_CLAUDE_AGENT" --variable environment="$ASK_CLAUDE_ENVIRONMENT" \
        --secrets-file <(printf 'api_key=%s\n' "$ANTHROPIC_API_KEY") list.hurl sessions.hurl

# the agents on this account (id, model, name), for --agent / ASK_CLAUDE_AGENT
agents:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; . "{{ justfile_directory() }}/.env"; set +a
    curl -sS 'https://api.anthropic.com/v1/agents?limit=100' \
        -H @<(printf 'x-api-key: %s\nanthropic-version: 2026-10-21\nanthropic-beta: managed-agents-2026-04-01\n' "$ANTHROPIC_API_KEY") |
        jq -r 'if .data then .data[] | [.id, (.model | if type == "object" then (.id // .name // tostring) else . end), .name] | @tsv else "just agents: \(.error.message // "unreadable response")\n" | halt_error end'

# the environments on this account (id, networking, name), for --environment / ASK_CLAUDE_ENVIRONMENT
environments:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; . "{{ justfile_directory() }}/.env"; set +a
    curl -sS 'https://api.anthropic.com/v1/environments?limit=100' \
        -H @<(printf 'x-api-key: %s\nanthropic-version: 2026-10-21\nanthropic-beta: managed-agents-2026-04-01\n' "$ANTHROPIC_API_KEY") |
        jq -r 'if .data then .data[] | [.id, (.config.networking.type // "-"), .name] | @tsv else "just environments: \(.error.message // "unreadable response")\n" | halt_error end'

# live OpenAI-compatible contract, one call. Reads .env like `test`: OPENAI_BASE_URL
# is the API root (e.g. https://api.venice.ai/api/v1), OPENAI_API_KEY the key,
# OPENAI_MODEL the model (default gemini-4-argon); the key goes to hurl on a pipe.
[working-directory: 'tests/providers']
test-openai:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a; . "{{ justfile_directory() }}/.env"; set +a
    hurl --test --variable base_url="$OPENAI_BASE_URL" --variable model="${OPENAI_MODEL:-gemini-4-argon}" \
        --secrets-file <(printf 'api_key=%s\n' "$OPENAI_API_KEY") openai-compatible.hurl

# ---- the Rust port (docs/plan.md phase 4; rust/) ------------------------
# cargo comes from mise's rust; the recipes run it through `mise exec` until
# the shims know the version (`mise use -g rust@1.99.1`).

# build the Rust binary (debug)
[group('rust')]
build:
    cd rust && mise exec rust -- cargo build

# the Rust unit tests: every lib/*.jq twin against the same goldens
[group('rust')]
rust-test:
    cd rust && mise exec rust -- cargo test

# fmt --check and clippy, the Rust side of `just lint`
[group('rust')]
lint:
    cd rust && mise exec rust -- cargo fmt --check && mise exec rust -- cargo clippy -- -D warnings

# criterion benchmarks over the recorded fixtures (rust/target/criterion/report)
[group('rust')]
bench:
    cd rust && mise exec rust -- cargo bench

# the two binaries agree on their interface: usage diff of the bash spec and the Rust one
[group('rust')]
parity: build
    #!/usr/bin/env bash
    set -euo pipefail
    usage diff ask-claude <(rust/target/debug/ask-claude --usage-spec)

# Skips pty like unit-tests, and bash-only: the tests that read the script as a
# file or pin what only the script does.

# the bats suite against the Rust binary (R5 onwards; bash stays the oracle until R8)
[group('rust')]
test-rust: build
    ASK_CLAUDE_BIN="{{ justfile_directory() }}/rust/target/debug/ask-claude" bats --filter-tags '!pty,!bash-only' tests/unit

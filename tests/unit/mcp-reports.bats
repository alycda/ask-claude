#!/usr/bin/env bats
# shellcheck disable=SC2016  # jq programs in single quotes mention $ on purpose
load helpers

# The MCP server's reports have jq as their oracle, the way the Rust port
# has claude.sh's jq: each tests/fixtures/mcp/reports/<name>.jq is a query
# from the README's "Events on disk", verbatim, and <name>.json is what it
# prints for tests/fixtures/mcp/events.jsonl (<name>.since.json: for the
# events at or after 2026-11-05). No server is needed, so this file runs
# in `just unit-tests`. Change a query in the README and here together,
# and regenerate its goldens with the commands in the tests below.

REPORTS="$FIX/mcp/reports"
EVENTS="$FIX/mcp/events.jsonl"
NAMES=(tokens_by_chat cache_hit_by_model seconds_by_model refusals_by_category advisor_spend)

# query <name>: the program without its comment header, whitespace squeezed
query() {
    local q
    q=$(grep -v '^#' "$REPORTS/$1.jq" | tr -s ' \n' '  ')
    q=${q# }
    printf '%s' "${q% }"
}

@test "mcp-reports: each query is the README's, verbatim" {
    local readme
    readme=$(tr -s ' \n' '  ' <"$ROOT/README.md")
    for n in "${NAMES[@]}"; do
        [[ $readme == *"jq -s '$(query "$n")' \"\$EVENTS\""* ]] || {
            echo "$n.jq is not a query in the README" >&2
            return 1
        }
    done
}

@test "mcp-reports: each golden is what its query prints for the fixture" {
    for n in "${NAMES[@]}"; do
        diff -u <(jq -S . "$REPORTS/$n.json") <(jq -s -f "$REPORTS/$n.jq" "$EVENTS" | jq -S .)
    done
}

@test "mcp-reports: each since golden is what its query prints for the events from 2026-11-05" {
    for n in "${NAMES[@]}"; do
        diff -u <(jq -S . "$REPORTS/$n.since.json") \
            <(jq -s --arg since 2026-11-05 'map(select(.ts >= $since))' "$EVENTS" | jq -f "$REPORTS/$n.jq" | jq -S .)
    done
}

@test "mcp-reports: the fixture's lines are event log lines, keys in the format's order" {
    # the key order of lib/event.jq, from a line the script's own tests write
    local order
    order=$(head -1 "$FIX/events/text.jsonl" | jq -c keys_unsorted)
    [ "$(jq -c keys_unsorted "$EVENTS" | sort -u)" = "$order" ]
}

#!/usr/bin/env bats
# shellcheck disable=SC2012  # ls counts files in a test, where find would add nothing
load helpers

STATE_DIR() { echo "$XDG_STATE_HOME/ask-claude"; }

# mkchat <name> <mtime YYYYMMDDhhmm> <turns-json>
mkchat() {
    mkdir -p "$(STATE_DIR)"
    printf '%s' "$3" > "$(STATE_DIR)/$1.json"
    touch -t "$2" "$(STATE_DIR)/$1.json"
}
TWO='[{"role":"user","content":"first question about something long enough to be cut at sixty characters exactly here"},{"role":"assistant","content":[{"type":"text","text":"answer"}]}]'
ONE='[{"role":"user","content":"second"}]'

@test "list: empty state prints nothing and exits 0" {
    run --separate-stderr "$BIN" list
    [ "$status" -eq 0 ]
    [ -z "$output" ]
}

@test "list: newest first, turn count, first prompt cut at 60" {
    mkchat 20260101T000000 202601010000 "$TWO"
    mkchat 20260102T000000 202601020000 "$ONE"
    run --separate-stderr "$BIN" list
    [ "$status" -eq 0 ]
    [ "${lines[0]%%  *}" = "20260102T000000" ]
    grep -q '1 turns' <<<"${lines[0]}"
    [ "${lines[1]%%  *}" = "20260101T000000" ]
    grep -q '2 turns' <<<"${lines[1]}"
    grep -q 'first question about something long enough to be cut at sixt$' <<<"${lines[1]}"
}

@test "resume: unknown name exits 2" {
    run --separate-stderr "$BIN" resume nope
    [ "$status" -eq 2 ]
    grep -q 'nothing to resume' <<<"$stderr"
}

@test "resume: nothing to resume exits 2" {
    run --separate-stderr "$BIN" resume
    [ "$status" -eq 2 ]
}

@test "resume: a prefix selects the chat and prints it first" {
    mkchat 20260101T000000 202601010000 "$TWO"
    run --separate-stderr bash -c "printf '/quit\n' | ASK_CLAUDE_REPLAY='$FIX/sse/text.sse' '$BIN' resume 202601"
    [ "$status" -eq 0 ]
    grep -q 'resuming' <<<"$stderr"
    grep -q '^you: first question' <<<"$stderr"
    grep -q '^claude: answer' <<<"$stderr"
}

@test "resume: without a name picks the newest chat that has turns" {
    mkchat 20260101T000000 202601010000 "$TWO"
    mkchat 20260102T000000 202601020000 '[]'
    run --separate-stderr bash -c "printf '/quit\n' | '$BIN' resume"
    [ "$status" -eq 0 ]
    grep -q 'resuming.*20260101T000000' <<<"$stderr"
}

@test "resume: a new turn is appended to the resumed chat, not a new file" {
    mkchat 20260101T000000 202601010000 "$TWO"
    bash -c "printf 'again\n/quit\n' | ASK_CLAUDE_REPLAY='$FIX/sse/text.sse' '$BIN' resume 20260101" >/dev/null 2>&1
    [ "$(ls "$(STATE_DIR)"/*.json | wc -l)" -eq 1 ]
    jq -e 'length==4 and .[2].content=="again"' "$(STATE_DIR)/20260101T000000.json"
}

@test "resume: a chat that used an advisor sends the advisor beta without --advisor" {
    # the API refuses advisor blocks in the history without the beta
    used=$(jq -c '[{role: "user", content: "q"}, {role: "assistant", content: .content}]' "$FIX/sse/advisor.message.json")
    mkchat 20260101T000000 202601010000 "$used"
    bash -c "printf 'again\n/quit\n' | ASK_CLAUDE_REPLAY='$FIX/sse/text.sse' ASK_CLAUDE_BETA_LOG='$TMP/betas' '$BIN' resume 20260101" >/dev/null 2>&1
    grep -q 'advisor-tool-2026-03-01' "$TMP/betas"
    rm -f "$TMP/betas"
    mkchat 20260102T000000 202601020000 "$TWO"
    bash -c "printf 'again\n/quit\n' | ASK_CLAUDE_REPLAY='$FIX/sse/text.sse' ASK_CLAUDE_BETA_LOG='$TMP/betas' '$BIN' resume 20260102" >/dev/null 2>&1
    [ -s "$TMP/betas" ]
    refute grep -q 'advisor' "$TMP/betas"
}

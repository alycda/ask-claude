#!/usr/bin/env bats
# shellcheck disable=SC2012,SC2059  # ls counts files in a test; a format string from the test itself
load helpers

chat() { # stdin script, then args
    printf "$1" | ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "${@:2}"
}
STATE_DIR() { echo "$XDG_STATE_HOME/ask-claude"; }

@test "chat: a line is a turn; /quit prints the history and a resume hint" {
    run --separate-stderr chat 'hi\n/quit\n'
    [ "$status" -eq 0 ]
    grep -q '^pong' <<<"$output"
    grep -q '^you: hi' <<<"$output"
    grep -q '^claude: pong' <<<"$output"
    grep -q 'resume with: .*resume 20' <<<"$stderr"
}

@test "chat: /history prints the chat so far" {
    run --separate-stderr chat 'hi\n/history\n/quit\n'
    [ "$(grep -c '^you: hi' <<<"$output")" -ge 2 ]
}

@test "chat: /new starts a second chat file" {
    run --separate-stderr chat 'hi\n/new\nhi\n/quit\n'
    [ "$status" -eq 0 ]
    grep -q 'new chat:' <<<"$stderr"
    [ "$(ls "$(STATE_DIR)"/*.json | wc -l)" -eq 2 ]
}

@test "chat: quitting an empty chat leaves no file" {
    run chat '/quit\n'
    [ "$status" -eq 0 ]
    [ -z "$(ls "$(STATE_DIR)"/*.json 2>/dev/null)" ]
}

@test "chat: blank lines are ignored" {
    run chat '\n\n/quit\n'
    [ "$status" -eq 0 ]
    [ -z "$(ls "$(STATE_DIR)"/*.json 2>/dev/null)" ]
}

@test "chat: off a terminal no prompt is printed" {
    run --separate-stderr chat 'hi
/quit
'
    [ "$status" -eq 0 ]
    refute grep -q '^>' <<<"$output"
    refute grep -q '^>' <<<"$stderr"
}

@test "chat: the end of input quits the way /quit does" {
    run --separate-stderr chat 'hi
'
    [ "$status" -eq 0 ]
    grep -q '^you: hi' <<<"$output"
    grep -q 'resume with: .*resume 20' <<<"$stderr"
}

@test "chat: a last line without a newline is not sent, as read leaves it" {
    run --separate-stderr chat 'hi
again'
    [ "$status" -eq 0 ]
    [ "$(grep -c '^you: ' <<<"$output")" -eq 1 ]
}

@test "chat: an empty prompt argument is no prompt, the chat loop and no request" {
    run --separate-stderr chat '/quit\n' ''
    [ "$status" -eq 0 ]
    grep -q '(/new resets, /quit exits)' <<<"$stderr"
    [ ! -s "$TMP/requests.jsonl" ]
}

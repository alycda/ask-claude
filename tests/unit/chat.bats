#!/usr/bin/env bats
load helpers

chat() { # stdin script
    printf "$1" | ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" "$BIN"
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

# bats test_tags=pty
@test "chat: the bash running the loop supports bracketed paste (>= 5.1)" {
    # usage's shebang resolves `bash` from PATH; the same resolution happens here
    bash -c '(( BASH_VERSINFO[0] > 5 || (BASH_VERSINFO[0] == 5 && BASH_VERSINFO[1] >= 1) ))'
    bash -ic 'bind -v' 2>/dev/null | grep -q 'enable-bracketed-paste on'
}

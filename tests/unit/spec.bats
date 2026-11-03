#!/usr/bin/env bats
load helpers

# The #USAGE block in ask-claude, checked with usage itself.

explain() { usage explain -f "$BIN" -- ask-claude "$@"; }

@test "spec: parses and lists both subcommands in --help" {
    run usage bash "$BIN" --help
    [ "$status" -eq 0 ]
    grep -qE '^\s+list\b'   <<<"$output"
    grep -qE '^\s+resume\b' <<<"$output"
}

@test "spec: every flag binds to its usage_ name" {
    run explain -m haiku -e high -S sys -s pptx --max-tokens 10 "hello there"
    [ "$status" -eq 0 ]
    grep -qE '^\s+flag\s+--model\s+haiku'        <<<"$output"
    grep -qE '^\s+flag\s+--effort\s+high'        <<<"$output"
    grep -qE '^\s+flag\s+--system\s+sys'         <<<"$output"
    grep -qE '^\s+flag\s+--skills\s+pptx'        <<<"$output"
    grep -qE '^\s+flag\s+--max-tokens\s+10'      <<<"$output"
    grep -qE '^\s+arg\s+\[prompt\]\s+hello there' <<<"$output"
    ! grep -q 'refused' <<<"$output"
}

@test "spec: defaults apply when flags are absent" {
    run explain "hi"
    grep -qE '^\s+flag\s+--model\s+claude-opus-5-5' <<<"$output"
    grep -qE '^\s+flag\s+--max-tokens\s+16000'      <<<"$output"
}

@test "spec: resume takes an optional chat name" {
    run explain resume
    grep -qE '^command\s+resume' <<<"$output"
    ! grep -q 'refused' <<<"$output"
    run explain resume 20260101T000000
    grep -qE '^\s+arg\s+\[chat\]\s+20260101T000000' <<<"$output"
}

@test "spec: list takes no argument" {
    run explain list
    grep -qE '^command\s+list' <<<"$output"
    run explain list extra
    grep -q 'refused' <<<"$output"
}

@test "spec: an unknown flag is refused" {
    run explain --nope
    grep -q 'refused' <<<"$output"
}

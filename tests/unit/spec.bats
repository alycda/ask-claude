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
    grep -qE '^command\s+(ask-claude\s+)?resume' <<<"$output"
    ! grep -q 'refused' <<<"$output"
    run explain resume 20260101T000000
    grep -qE '^\s+arg\s+\[chat\]\s+20260101T000000' <<<"$output"
}

@test "spec: list takes no argument" {
    run explain list
    grep -qE '^command\s+(ask-claude\s+)?list' <<<"$output"
    run explain list extra
    grep -q 'refused' <<<"$output"
}

@test "spec: an unknown flag is refused" {
    run explain --nope
    grep -q 'refused' <<<"$output"
}

@test "version: -V and --version print the script's copy of VERSION and exit 0" {
    run "$BIN" -V
    [ "$status" -eq 0 ]
    [ "$output" = "ask-claude $(cat "$ROOT/VERSION")" ]
    run "$BIN" --version
    [ "$status" -eq 0 ]
    [ "$output" = "ask-claude $(cat "$ROOT/VERSION")" ]
}

# bats test_tags=bash-only
@test "version: the script's copy matches VERSION (just check-version-line)" {
    grep -qx "readonly ASK_CLAUDE_VERSION=$(cat "$ROOT/VERSION")" "$BIN"
}

@test "completions: usage generates a script for each shell from the spec" {
    usage generate completion zsh  ask-claude -f "$BIN" | grep -q '^#compdef ask-claude$'
    usage generate completion bash ask-claude -f "$BIN" | grep -q 'complete .*ask-claude'
    usage generate completion fish ask-claude -f "$BIN" | grep -q 'complete .*ask-claude'
}

# agent: a Managed Agents session (docs/plans/2026-11-05-001 U3)

@test "agent spec: the command, its prompt and its subcommands bind" {
    run explain agent "hello there"
    grep -qE '^command\s+(ask-claude\s+)?agent' <<<"$output"
    grep -qE '^\s+arg\s+\[prompt\]\s+hello there' <<<"$output"
    ! grep -q refused <<<"$output"
    for sub in list "resume sesn_x" "files sesn_x" "files sesn_x a.txt b.txt" "budget sesn_x 250" "archive sesn_x" "trace sesn_x"; do
        # shellcheck disable=SC2086
        run explain agent $sub
        grep -qE "^command\s+(ask-claude\s+)?agent ${sub%% *}" <<<"$output"
        ! grep -q refused <<<"$output"
    done
    run explain agent list extra
    grep -q refused <<<"$output"
}

@test "agent spec: every flag binds, with its env fallback and default" {
    run explain agent --agent agent_x --environment env_x --budget 5 --vault vlt_a --vault vlt_b --repo owner/name@main --memory mem_1 --memory mem_2:ro --force "hi"
    grep -qE '^\s+flag\s+--agent\s+agent_x' <<<"$output"
    grep -qE '^\s+flag\s+--environment\s+env_x' <<<"$output"
    grep -qE '^\s+flag\s+--budget\s+5' <<<"$output"
    grep -qE '^\s+flag\s+--vault\s+vlt_a' <<<"$output"
    grep -qE '^\s+flag\s+--repo\s+owner/name@main' <<<"$output"
    grep -qE '^\s+flag\s+--memory\s+mem_1' <<<"$output"
    grep -qE '^\s+flag\s+--force' <<<"$output"
    ! grep -q refused <<<"$output"
    run explain agent "hi"
    grep -qE '^\s+flag\s+--budget\s+100\s+default' <<<"$output"
    ASK_CLAUDE_AGENT=agent_env ASK_CLAUDE_ENVIRONMENT=env_env run explain agent "hi"
    grep -qE '^\s+flag\s+--agent\s+agent_env' <<<"$output"
    grep -qE '^\s+flag\s+--environment\s+env_env' <<<"$output"
}

@test "agent spec: -f and the request flags reach the agent command" {
    run explain -f notes.md agent "summarize"
    grep -qE '^\s+flag\s+--file\s+notes.md' <<<"$output"
    ! grep -q refused <<<"$output"
}

@test "agent spec: the only break since 0.1.4 is the word agent, now a command" {
    # same file name on both sides: usage names the spec after the file
    mkdir -p "$TMP/old" "$TMP/new"
    jj file show -r v0.1.4 ask-claude > "$TMP/old/ask-claude" 2>/dev/null || skip "no v0.1.4 bookmark here"
    cp "$BIN" "$TMP/new/ask-claude"
    run usage diff "$TMP/old/ask-claude" "$TMP/new/ask-claude"
    [ "$status" -ne 0 ]
    grep -q 'Found 1 breaking' <<<"$output"
    grep -qE "^breaking \\[cmd-shadows-arg\\] at ask-claude: command 'agent'" <<<"$output"
}

@test "spec: --web and --mcp bind, --mcp repeats" {
    run explain --web --mcp https://a.example/mcp --mcp https://b.example/mcp "hi"
    grep -qE '^\s+flag\s+--web' <<<"$output"
    grep -qE '^\s+flag\s+--mcp\s+https://a.example/mcp' <<<"$output"
    ! grep -q refused <<<"$output"
}

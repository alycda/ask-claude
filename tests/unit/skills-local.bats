#!/usr/bin/env bats
# bats file_tags=rust-only
load helpers

# Local skills (CONCEPTS.md, Local skill and Skills index): U13 of
# docs/plans/2026-11-06-0143-feat-rust-parity-and-local-tools-plan.md, KTD14.
# With --tools, every skill discovered under .claude/skills/ in the repository
# and ~/.claude/skills/ is listed in `system` by name, description and
# when_to_use; a body reaches the model only as the result of a `skill` call,
# which passes the confirmation gate like any Local tool.
#
# What these tests pin beyond the plan's prose, so the feat commit can match:
#   - the model calls `skill` with {"name": "<skill>"}, as the fixture stream
#     tests/fixtures/sse/skill-call.sse does (toolu_FIXTURE01);
#   - the answer is the tool_result for toolu_FIXTURE01 in the second request
#     body, its content either a string or text blocks;
#   - stderr notes name the skill or root they are about; their wording is free.
# What they leave free: the delimiters and layout of the index in `system`,
# which tests/fixtures/request/local-tools-index.json pins once it is written.
#
# Precedence is the plan's stated default, personal over project, until the
# documentation read D7 asks for settles it. The plugin scenarios of U13 are
# written with U14's test commit, since activating a plugin needs
# `plugin enable`.
#
# The fixture tree, tests/fixtures/skills/, has no `.claude` directory name, so
# no tool that discovers skills in a subdirectory of this repository loads it;
# skills_tree copies it into a temporary repository and home.

SKILLS="$FIX/skills"

# skills_tree: a repository at $TMP/repo holding the fixture's project skills
# in .claude/skills/, a home at $TMP/home holding its personal skills, neither
# writable by group or others (KTD14 skips a root another account can write);
# then cd into the repository, the root the tools work in
skills_tree() {
    mkdir -p "$TMP/repo/.claude" "$TMP/home/.claude"
    cp -R "$SKILLS/project" "$TMP/repo/.claude/skills"
    cp -R "$SKILLS/personal" "$TMP/home/.claude/skills"
    chmod -R go-w "$TMP/repo" "$TMP/home"
    export HOME="$TMP/home"
    cd "$TMP/repo" || return
}

# skill <name> <frontmatter lines...>: write a project skill named <name>
skill() {
    local name="$1"
    shift
    mkdir -p ".claude/skills/$name"
    { printf -- '---\nname: %s\n' "$name"; printf '%s\n' "$@" '---' "$name body"; } >".claude/skills/$name/SKILL.md"
}

# one_shot <args...>: a one-shot replaying text.sse, every request body logged
one_shot() {
    ASK_CLAUDE_REPLAY="$FIX/sse/text.sse" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" "$BIN" "$@"
}

# call_skill <name>: a --tools one-shot whose first reply calls skill with
# {"name": <name>} and whose second ends the turn; ASK_CLAUDE_CONFIRM is
# allow unless the caller sets CONFIRM. Fails when skill-call.sse no longer
# carries the name it replaces, so no test passes without calling skill.
call_skill() {
    mkdir -p "$TMP/replay"
    sed "s/\"partial_json\":\"nested-meta/\"partial_json\":\"$1/" "$FIX/sse/skill-call.sse" >"$TMP/replay/1.sse"
    grep -q "\"partial_json\":\"$1" "$TMP/replay/1.sse" || { echo "call_skill: no skill name in skill-call.sse"; return 1; }
    cp "$FIX/sse/text.sse" "$TMP/replay/2.sse"
    ASK_CLAUDE_REPLAY="$TMP/replay" ASK_CLAUDE_REQUEST_LOG="$TMP/requests.jsonl" \
        ASK_CLAUDE_CONFIRM="${CONFIRM:-allow}" "$BIN" --tools "load it" </dev/null
}

request() { sed -n "$1p" "$TMP/requests.jsonl"; }
requests() { wc -l <"$TMP/requests.jsonl" | tr -d ' '; }
# system: the first request's system prompt as text (KTD12 makes it a string)
system() { request 1 | jq -r '.system // "" | if type == "array" then map(.text // "") | join("\n") else . end'; }
# result: the tool_result answering toolu_FIXTURE01, in the second request
result() { request 2 | jq -c '[.messages[-1].content[]? | select(.type == "tool_result" and .tool_use_id == "toolu_FIXTURE01")] | .[0]'; }
result_text() { result | jq -r '.content | if type == "string" then . else map(.text // "") | join("") end'; }
is_error() { result | jq -e '.is_error == true' >/dev/null; }

@test "skills-local: the index and the skill tool are sent only with --tools" {
    skills_tree
    run --separate-stderr one_shot "ping"
    [ "$status" -eq 0 ]
    refute grep -qF 'Plain fixture skill' "$TMP/requests.jsonl"
    request 1 | jq -e '[.tools[]?.name] | index("skill") == null'
    rm "$TMP/requests.jsonl"

    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    grep -qF 'Plain fixture skill' <<<"$(system)"
    request 1 | jq -e '[.tools[]?.name] | index("skill") != null'
}

@test "skills-local: the index holds name, description and when_to_use, and nothing from a body" {
    skills_tree
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    s="$(system)"
    grep -qF 'plain' <<<"$s"
    grep -qF 'Plain fixture skill, a scalar description, for the index test.' <<<"$s"
    grep -qF 'WHEN-MARKER' <<<"$s"
    grep -qF 'nested-meta' <<<"$s"
    refute grep -qF 'PLAIN-BODY-MARKER' "$TMP/requests.jsonl"
    refute grep -qF 'NESTED-BODY-MARKER' "$TMP/requests.jsonl"
    refute grep -qF 'REFERENCE-MARKER' "$TMP/requests.jsonl"
}

# Supabase's agent skills carry this shape: a quoted description, a license,
# and a nested metadata mapping a scalars-only subset parser would reject.
@test "skills-local: a frontmatter with a nested metadata mapping is indexed and none of the mapping is sent" {
    skills_tree
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    grep -qF "Nested metadata fixture: a quoted description, a license and a metadata mapping, the shape of Supabase's agent skills." <<<"$(system)"
    refute grep -qF 'METADATA-MARKER' "$TMP/requests.jsonl"
    refute grep -qF 'nested-meta' <<<"$stderr"
}

@test "skills-local: a skill call returns the body after the skill's canonical directory" {
    skills_tree
    dir="$(cd .claude/skills/nested-meta && pwd -P)"
    run --separate-stderr call_skill nested-meta
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 2 ]
    refute is_error
    t="$(result_text)"
    [[ "$t" == *"$dir"*NESTED-BODY-MARKER* ]]
}

@test "skills-local: a skill result keeps a relative reference as written and does not inline it" {
    skills_tree
    run --separate-stderr call_skill nested-meta
    [ "$status" -eq 0 ]
    t="$(result_text)"
    grep -qF 'references/rule-a.md' <<<"$t"
    refute grep -qF 'REFERENCE-MARKER' "$TMP/requests.jsonl"
}

@test "skills-local: a skill result carries no frontmatter" {
    skills_tree
    run --separate-stderr call_skill nested-meta
    [ "$status" -eq 0 ]
    t="$(result_text)"
    grep -qF 'NESTED-BODY-MARKER' <<<"$t"
    refute grep -qF 'METADATA-MARKER' <<<"$t"
    refute grep -qE '^license: MIT$' <<<"$t"
}

@test "skills-local: a skill call for an unknown name returns an error result" {
    skills_tree
    run --separate-stderr call_skill no-such-skill
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 2 ]
    is_error
}

@test "skills-local: a denied skill call returns an error result and sends no body" {
    skills_tree
    CONFIRM=deny run --separate-stderr call_skill nested-meta
    [ "$status" -eq 0 ]
    [ "$(requests)" -eq 2 ]
    is_error
    refute grep -qF 'NESTED-BODY-MARKER' "$TMP/requests.jsonl"
}

@test "skills-local: a description over the cap is cut and noted on stderr" {
    skills_tree
    skill long-desc "description: $(printf 'a%.0s' {1..1600})CUT-MARKER$(printf 'b%.0s' {1..100})"
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    s="$(system)"
    grep -qF "$(printf 'a%.0s' {1..1000})" <<<"$s"
    refute grep -qF 'CUT-MARKER' "$TMP/requests.jsonl"
    refute grep -qF 'bbbb' "$TMP/requests.jsonl"
    grep -qF 'long-desc' <<<"$stderr"
}

# The code point is written by the test, never tracked: KTD16 keeps every
# tracked fixture free of the nine bidi formatting characters.
@test "skills-local: a description carrying U+202E is indexed without it" {
    skills_tree
    rlo="$(printf '\342\200\256')"
    skill bidi "description: BIDI-BEFORE${rlo}BIDI-AFTER"
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    s="$(system)"
    grep -qF 'BIDI-BEFOREBIDI-AFTER' <<<"$s"
    refute grep -qF "$rlo" "$TMP/requests.jsonl"
    refute grep -qiF '‮' "$TMP/requests.jsonl"
}

@test "skills-local: a SKILL.md symlinked outside the repository is not indexed and loading it is an error" {
    skills_tree
    mkdir -p "$TMP/outside/escape" .claude/skills/escape
    printf -- '---\nname: escape\ndescription: OUTSIDE-MARKER\n---\nOUTSIDE-BODY\n' >"$TMP/outside/escape/SKILL.md"
    ln -s "$TMP/outside/escape/SKILL.md" .claude/skills/escape/SKILL.md
    run --separate-stderr call_skill escape
    [ "$status" -eq 0 ]
    refute grep -qF 'OUTSIDE-MARKER' "$TMP/requests.jsonl"
    is_error
    refute grep -qF 'OUTSIDE-BODY' "$TMP/requests.jsonl"
}

@test "skills-local: a skill whose name differs from its directory is skipped with a note" {
    skills_tree
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    refute grep -qF 'MISNAMED-MARKER' "$TMP/requests.jsonl"
    refute grep -qF 'not-misnamed' <<<"$(system)"
    grep -qF 'misnamed' <<<"$stderr"
}

# The plan's default until D7 settles it: personal beats project.
@test "skills-local: a personal skill shadows a project skill of the same name, with one stderr line" {
    skills_tree
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    s="$(system)"
    grep -qF 'PERSONAL-SHADOW-MARKER' <<<"$s"
    refute grep -qF 'PROJECT-SHADOW-MARKER' "$TMP/requests.jsonl"
    [ "$(grep -c 'shadowed' <<<"$stderr")" -eq 1 ]
    rm "$TMP/requests.jsonl"

    run --separate-stderr call_skill shadowed
    [ "$status" -eq 0 ]
    grep -qF 'PERSONAL-SHADOW-BODY' <<<"$(result_text)"
    refute grep -qF 'PROJECT-SHADOW-BODY' "$TMP/requests.jsonl"
}

@test "skills-local: a skills root that group or others can write is skipped with a note" {
    skills_tree
    chmod g+w .claude/skills
    ASK_CLAUDE_CONFIRM=allow run --separate-stderr one_shot --tools "ping" </dev/null
    [ "$status" -eq 0 ]
    refute grep -qF 'Plain fixture skill' "$TMP/requests.jsonl"
    grep -qF 'PERSONAL-SHADOW-MARKER' <<<"$(system)"
    grep -qF "$TMP/repo/.claude/skills" <<<"$stderr" || grep -qF "$(pwd -P)/.claude/skills" <<<"$stderr"
}

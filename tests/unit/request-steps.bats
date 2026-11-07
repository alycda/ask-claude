#!/usr/bin/env bats
# shellcheck disable=SC2016  # a jq program in single quotes mentions $ on purpose
load helpers

# lib/request.jq and lib/skills.jq one part at a time, like
# reassemble-steps.bats. Each test checks only the keys its step adds, with
# expected values taken from the arguments, so a step turns green as soon as
# its own part is written.
#
#   bats --filter-tags step:request_base tests/unit/request-steps.bats
#
# Steps, in order: request_base, request_system, request_effort,
# request_skills, request_container, skills_entries, skills_type. The goldens
# in request.bats stay the gate (they also check key order).

SKILLS='[{"skill_id":"pptx","version":"latest","type":"anthropic"},{"skill_id":"skill_abc","version":"skver_1","type":"custom"}]'
HIST="$FIX/request/history-one-turn.json"

# body <sys> <eff> <skills> <cid>: lib/request.jq with claude.sh's argument names
body() {
    jq -n --arg m claude-opus-5-5 --argjson max 16000 \
        --arg sys "$1" --arg eff "$2" --arg skills "$3" --arg cid "$4" \
        --slurpfile msgs "$HIST" -f "$ROOT/lib/request.jq"
}

# check <json> <jq-expr>: true for the JSON, with $hist and $skills bound
check() {
    jq -e --slurpfile hist "$HIST" --argjson skills "$SKILLS" "$2" <<<"$1" >/dev/null || {
        echo "check failed: $2" >&2
        echo "got: $1" >&2
        return 1
    }
}

# skills <text>: lib/skills.jq over raw text, the way claude.sh called it
skills() { printf '%s' "$1" | jq -R -s -c -f "$ROOT/lib/skills.jq"; }

# --- request -----------------------------------------------------------------

# bats test_tags=step:request_base
@test "step request_base: model, max_tokens, history, fallbacks, stream, cache" {
    check "$(body "" "" "" "")" '
        .model == "claude-opus-5-5" and .max_tokens == 16000
        and .messages == $hist[0] and .fallbacks == "default"
        and .stream == true and .cache_control == {type: "ephemeral"}'
}

# bats test_tags=step:request_system
@test "step request_system: system only when given" {
    check "$(body "be brief" "" "" "")" '.system == "be brief"'
    check "$(body "" "" "" "")" 'has("system") | not'
}

# bats test_tags=step:request_effort
@test "step request_effort: output_config.effort only when given" {
    check "$(body "" low "" "")" '.output_config == {effort: "low"}'
    check "$(body "" "" "" "")" 'has("output_config") | not'
}

# bats test_tags=step:request_skills
@test "step request_skills: skills add the code_execution tool and a container" {
    check "$(body "" "" "$SKILLS" "")" '
        .tools == [{type: "code_execution_20260521", name: "code_execution"}]
        and .container == {skills: $skills}'
    check "$(body "" "" "" "")" '(has("tools") or has("container")) | not'
}

# bats test_tags=step:request_container
@test "step request_container: a known container id is sent with the skills" {
    check "$(body "" "" "$SKILLS" container_FIXTURE)" '
        .container.id == "container_FIXTURE" and .container.skills == $skills'
}

# --- skills ------------------------------------------------------------------

# bats test_tags=step:skills_entries
@test "step skills_entries: one entry per non-empty line, version defaults to latest" {
    check "$(skills $'pptx\nskill_abc@skver_1\n')" '
        map({skill_id, version})
        == [{skill_id: "pptx", version: "latest"},
            {skill_id: "skill_abc", version: "skver_1"}]'
    check "$(skills '')" '. == []'
}

# bats test_tags=step:skills_type
@test "step skills_type: skill_ ids are custom, the rest anthropic" {
    check "$(skills $'pptx\nskill_abc@skver_1')" 'map(.type) == ["anthropic", "custom"]'
}

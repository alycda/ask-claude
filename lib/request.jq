# lib/request.jq: build the Messages API request body.
#
# Invocation (see tests/unit/request.bats), with claude.sh's argument names:
#     jq -n --arg m MODEL --argjson max N --arg sys SYSTEM --arg eff EFFORT \
#           --arg skills SKILLS_JSON --arg cid CONTAINER_ID \
#           --slurpfile msgs history.json -f lib/request.jq
#
# -n means there is no input: everything comes from the variables.
#   $m, $sys, $eff, $skills, $cid   strings; "" means "not given"
#   $max                            a number (--argjson parses it)
#   $msgs                           an ARRAY of the file's JSON values
#                                   (--slurpfile), so the history is $msgs[0]
#   $skills                         JSON text from lib/skills.jq; fromjson parses it
#
# The tests compare with tests/fixtures/request/*.json byte for byte, so key
# order matters: `A + B` keeps A's keys first, then adds B's. Build the base
# object, then add each optional part only when its variable is not "".
#
# Shape:
#
#     BASE
#     + (if $sys != "" then {...} else {} end)
#     + ...

# BASE: every request has these six keys, in this order. Each value is either
# one of the variables or a constant:
#   model          $m           the model id (--arg m)
#   max_tokens     $max         a number, because --argjson parsed it
#   messages       $msgs[0]     the whole chat history. --slurpfile wraps the
#                               file in an array, so [0] unwraps it
#   fallbacks      "default"    let the API retry a refusal on another model
#   stream         true         always SSE (lib/stream.jq prints it)
#   cache_control  {type: "ephemeral"}   cache the growing conversation
{
    model: $m,
    max_tokens: $max,
    messages: $msgs[0],
    fallbacks: "default",
    stream: true,
    cache_control: {type: "ephemeral"}
}
# system: the system prompt, only when one was given. When $sys is "" the
# else branch adds {}, which adds nothing, so the key is left out.
+ (if $sys != "" then {system: $sys} else {} end)
+ (if $eff != "" then {output_config: {effort: $eff}} else {} end)
# TODO: when $skills is not "":
#       tools: [{type: "code_execution_20260521", name: "code_execution"}]
#       container: {skills: ($skills | fromjson)}, plus id: $cid when $cid is not ""
+ {}

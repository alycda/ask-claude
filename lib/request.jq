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

# TODO BASE, in this key order: model ($m), max_tokens ($max), messages
#      ($msgs[0]), fallbacks ("default"), stream (true),
#      cache_control ({type: "ephemeral"})
{}
# TODO: system ($sys) when $sys is not ""
+ {}
# TODO: output_config: {effort: $eff} when $eff is not ""
+ {}
# TODO: when $skills is not "":
#       tools: [{type: "code_execution_20260521", name: "code_execution"}]
#       container: {skills: ($skills | fromjson)}, plus id: $cid when $cid is not ""
+ {}

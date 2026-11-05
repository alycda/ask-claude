# lib/session-create.jq: build the body of POST /v1/sessions.
#
# Invocation (see tests/unit/agent.bats, tests/fixtures/agents/create-*.json):
#     jq -n -c --arg agent AGENT_ID --arg env ENV_ID --arg budget CENTS \
#           --arg title TITLE --arg prompt PROMPT -f lib/session-create.jq
#
#   $budget   whole cents as a string; "0" means no budget at all
#   $title    "" leaves the title out
#   $file     a Files API id to attach as a resource; "" attaches nothing
#   $file_name  its name: the file is mounted at /mnt/session/uploads/<name>
#   $prompt   the first user.message, sent as initial_events so the session
#             is created running and one request does both

{agent: {type: "agent", id: $agent}, environment_id: $env}
+ (if $title != "" then {title: $title} else {} end)
+ (if $budget != "0" then {budget: {type: "limit", max_list_cost: {amount: $budget, currency: "USD"}}} else {} end)
+ (if $file != "" then {resources: [{type: "file", file_id: $file, mount_path: ("/mnt/session/uploads/" + $file_name)}]} else {} end)
+ {initial_events: [{type: "user.message", content: [{type: "text", text: $prompt}]}]}

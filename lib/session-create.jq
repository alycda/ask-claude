# lib/session-create.jq: build the body of POST /v1/sessions.
#
# Invocation (see tests/unit/agent.bats, tests/fixtures/agents/create-*.json):
#     jq -n -c --arg agent AGENT_ID --arg env ENV_ID --arg budget CENTS \
#           --arg title TITLE --arg prompt PROMPT --arg file FILE_ID \
#           --arg file_name NAME --arg vaults "vlt_a vlt_b" \
#           --arg repo "owner/name@branch" --arg memories "memstore_a memstore_b:ro" \
#           -f lib/session-create.jq
#
#   $budget     whole cents as a string; "0" means no budget at all
#   $title      "" leaves the title out
#   $prompt     the first user.message, sent as initial_events so the session
#               is created running and one request does both
#   $file       a Files API id to attach as a resource; "" attaches nothing
#   $file_name  its name: the file is mounted at /mnt/session/uploads/<name>
#   $vaults     vault ids, space-separated; "" attaches none
#   $repo       owner/name, or a https://github.com URL, with an optional
#               @branch; mounted at /workspace/<name>. The token comes from
#               the environment (env.GITHUB_TOKEN), never from an argument,
#               so it is not on jq's command line
#   $memories   memory store ids, space-separated, :ro for read-only

def words: split(" ") | map(select(length > 0));
def repo_resource:
  ($repo | capture("^(?<url>(https://github\\.com/)?(?<owner>[^/@]+)/(?<name>[^/@]+))(@(?<branch>.+))?$")) as $r
  | {type: "github_repository",
     url: (if ($r.url | startswith("https://")) then $r.url else "https://github.com/" + $r.url end),
     mount_path: ("/workspace/" + $r.name),
     authorization_token: env.GITHUB_TOKEN}
    + (if $r.branch != null then {checkout: {type: "branch", name: $r.branch}} else {} end);
def memory_resource:
  split(":") | {type: "memory_store", memory_store_id: .[0]}
  + (if .[1] == "ro" then {access: "read_only"} else {} end);

{agent: {type: "agent", id: $agent}, environment_id: $env}
+ (if $title != "" then {title: $title} else {} end)
+ (if $budget != "0" then {budget: {type: "limit", max_list_cost: {amount: $budget, currency: "USD"}}} else {} end)
+ (if $vaults != "" then {vault_ids: ($vaults | words)} else {} end)
+ (([ (if $file != "" then {type: "file", file_id: $file, mount_path: ("/mnt/session/uploads/" + $file_name)} else empty end),
      (if $repo != "" then repo_resource else empty end) ]
    + ($memories | words | map(memory_resource))) as $resources
   | if ($resources | length) > 0 then {resources: $resources} else {} end)
+ {initial_events: [{type: "user.message", content: [{type: "text", text: $prompt}]}]}

# Needs validation

Prioritised leads with no severity. Do not send audit traffic to a deployment to resolve them; use the owner-observed checks below.

## State-dir gate ignores a failed chmod and never checks the owner, so a directory another account owns is accepted and its planted .always.<pid> files auto-allow tool confirmations

`ask-claude.sh:state-dir/owner-and-parent-unchecked`

The gate at ask-claude.sh:113-120 is meant to refuse a $STATE in which other accounts can plant names. It discards the status of `mkdir -p "$STATE" && chmod 700 "$STATE"` and then checks only the group/other write bits of the resolved directory (ls -ldnL follows a symlink). A $STATE that another account owns with mode 755, or a $STATE path that is a symlink to such a directory, passes the gate. The owner of that directory can pre-create $STATE/.always.<N> for the whole PID range. session_confirm (434-437) returns allow when $STATE/.always.$$ exists, whoever created it, and it does this before the no-TTY deny at 438-442. Every tool confirmation of the victim's Managed Agents session is then answered allow without a prompt, including in unattended runs that would otherwise deny. The same owner also controls .stream.$$ (480/484) and sessions/ entries. Exploitation needs the victim's XDG_STATE_HOME (or HOME) to resolve to a location where another local account can create or own the ask-claude directory.

**Claimed root cause.** ask-claude.sh:113 runs `mkdir -p "$STATE" && chmod 700 "$STATE"` and ignores the result (set -uo pipefail, no -e). Lines 118-120 test only the group/other write bits from `ls -ldnL | cut -c6,9`. There is no owner/uid comparison, no refusal of a symlinked $STATE, and no check of parent directories. $STATE honours XDG_STATE_HOME (line 66), so it can resolve outside $HOME. session_confirm (434) treats the existence of a fixed-name file in $STATE as a standing allow grant, without checking who created it.

### Trace

1. **entrypoint** `ask-claude.sh:66` (STATE): STATE is ${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude, from the environment
2. **propagation** `ask-claude.sh:113` (state-dir gate): mkdir -p && chmod 700; a chmod failure on a directory another account owns is ignored
3. **propagation** `ask-claude.sh:118` (state-dir gate): Only group/other write bits of the symlink-resolved directory are tested; owner, symlink and parents are not checked
4. **sink** `ask-claude.sh:434` (session_confirm): An existing $STATE/.always.$$ yields allow without a prompt, before the no-TTY deny at 438

### Verified evidence

- `ask-claude.sh:113`: Unchecked mkdir/chmod result
- `ask-claude.sh:118`: Write-bit-only check via ls -ldnL | cut -c6,9
- `ask-claude.sh:434`: File existence is treated as an allow grant, checked before the TTY check
- `ask-claude.sh:438`: No-TTY deny comes after the .always check, so a planted grant overrides it
- `ask-claude.sh:447`: The legitimate always grant is created with : > and never removed or owner-checked
- `ask-claude.sh:480`: The stream log is a fixed name in $STATE, truncated with : > at 484

### Blockers

- The parent sandbox runs as a single uid (1000), so a second account that owns $STATE and plants .always.<pid> cannot be staged. Locally verified in parts: the real script passes the gate for a uid-0 mode-755 $STATE, and a pre-existing .always.<pid> that the current run did not create turns a no-TTY deny into allow. The two-account chain end to end is not observed.
- Whether a real user sets XDG_STATE_HOME (or HOME) to a path where another local account can create, own, or symlink the ask-claude directory (for example under /tmp or a shared or network home) is a deployment fact outside source. Without it there is no second principal and no impact.

### Bounded local plan

On a disposable multi-user Linux VM with no network: as account B, mkdir -m 755 /tmp/xs /tmp/xs/ask-claude and touch /tmp/xs/ask-claude/.always.{1..99999}. As account A, with the agents/v08 fixtures copied to /home/A/fx (1.sse ends requires_action with an agent.tool_use for bash; 2.sse ends end_turn), run: usage_model=m usage_max_tokens=16000 usage_cmd=agent usage_agent=agent_X usage_environment=env_X usage_budget=0 usage_prompt=hi ANTHROPIC_API_KEY=sk-ant-test-not-a-key ASK_CLAUDE_REPLAY=/home/A/fx ASK_CLAUDE_REQUEST_LOG=/home/A/reqlog XDG_STATE_HOME=/tmp/xs bash ask-claude.sh </dev/null. Expected if vulnerable: stderr shows 'ask-claude: allowed bash' and reqlog has a user.tool_confirmation with result allow. Control: the same command with an A-owned mode-700 state dir with no .always files prints 'denied ... stdin is not a terminal'. Regression for the fix: the gate must die when stat -c %u "$STATE" differs from id -u, when $STATE is a symlink, or when mkdir/chmod fails.

### Owner-observed check

The owner checks whether any machine or shared host where ask-claude runs sets XDG_STATE_HOME or HOME to a path where another local account can create directories or symlinks (for example a world-writable /tmp subtree or a shared home), and reports the path and its parent ownership and mode. No audit traffic is sent anywhere.

## One ASK_CLAUDE_MCP_TOKEN is attached to every --mcp server, so each listed server operator can receive the credential meant for another

`lib/request.jq:mcp_servers:shared-authorization-token`

When --mcp is given more than once and ASK_CLAUDE_MCP_TOKEN is set, lib/request.jq attaches the same token as authorization_token to every mcp_servers entry in the Messages request. If the provider's MCP connector presents authorization_token to each server URL as its bearer credential, the operator of any listed server (for example a public MCP server used next to a private one) receives the token issued for the other server and can act as the user against it. The CLI has no per-server token mapping and gives no warning when one token meets several servers. The attacker is limited to the operator of an MCP server the user chose to list; the README documents the token as applying to 'the --mcp servers', so the fan-out is documented but the credential audience is never narrowed.

**Claimed root cause.** lib/request.jq:58-59 builds every mcp_servers entry from the space-separated $mcp list and adds authorization_token: env.ASK_CLAUDE_MCP_TOKEN to each one, with no per-server token mapping. ask-claude.sh:101-103 checks only that each --mcp value starts with https://.

### Trace

1. **entrypoint** `ask-claude.sh:101` (--mcp gate): Each user-supplied --mcp value (usage_mcp) is checked only for an https:// prefix; any number of servers is accepted.
2. **propagation** `ask-claude.sh:276` (call()): The space-joined URL list is passed to lib/request.jq as --arg mcp.
3. **propagation** `lib/request.jq:59` (mcp_servers): Every server object built from $mcp gets authorization_token: env.ASK_CLAUDE_MCP_TOKEN when the variable is non-empty.
4. **sink** `ask-claude.sh:251` (transport()): The body is sent with curl to the Messages API; the provider's MCP connector then connects to each listed URL with the attached token (provider side, not observable offline).

### Verified evidence

- `lib/request.jq:59`: The same environment token is added to every mcp_servers entry; there is no per-URL token source.
- `ask-claude.sh:17`: The --mcp flag is repeatable, and its help says 'ASK_CLAUDE_MCP_TOKEN for its auth' (singular) with no scoping.
- `README.md:33`: The README documents the variable as the 'authorization token for the --mcp servers', with no per-server scoping or warning.

### Blockers

- Whether the provider's MCP connector sends each entry's authorization_token to that entry's server URL (for example as 'Authorization: Bearer <token>') is provider-side behaviour and cannot be observed offline. Locally it is established only that the request body binds the second server to the same token as the first (agents/v10/artifacts/mcp2-result.txt).

### Bounded local plan

Done, bounded and offline: in the sandbox, ASK_CLAUDE_REPLAY=tests/fixtures/sse/text.sse, ASK_CLAUDE_REQUEST_LOG=/scratch/out/req.jsonl, ASK_CLAUDE_MCP_TOKEN=dummy_token_for_private_A, usage_mcp='https://private-a.example/mcp https://public-b.example/mcp', usage_prompt=ping, bash /target/ask-claude.sh; then jq -c '{mcp_servers,tools}' on the log. Result: both mcp_servers entries (private-a-example and public-b-example) carry authorization_token dummy_token_for_private_A.

### Owner-observed check

The owner runs one live one-shot with ASK_CLAUDE_MCP_TOKEN=dummy-not-a-secret and two --mcp URLs, both owner-controlled https endpoints that log request headers and accept the dummy credential. Confirm whether the second endpoint receives 'Authorization: Bearer dummy-not-a-secret'. Use only dummy credentials and owner-controlled endpoints.


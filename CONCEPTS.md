# Concepts

Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound captures and refreshes learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## Transports

### Messages path
The transport that sends one request per turn to the Messages API and keeps the conversation on this machine as a Chat. The reply arrives as a Stream; the request carries the whole Chat so far.

### Session
A Managed Agents session: the second transport, in which the API runs an Agent inside an Environment and keeps the transcript itself. A Session is created with a Budget, carries Resources, and a turn ends when the stream goes idle, not when a message stops. Nothing of its history is kept locally beyond what Resume needs.

### Provider
Which API shape a request takes: the Messages shape, or the OpenAI chat-completions shape that other vendors also serve. A vendor that speaks an existing shape is reached through a Base URL, not by adding a Provider.
*Avoid:* calling Venice a provider.

### Base URL
The API root a Provider's requests are sent to. It must be https, or http on a loopback address, so the key never crosses a network in clear.

### Replay seam
The test seam that substitutes a recorded Stream for the network, so every behaviour test runs offline. Its companion, the Request log, records each request body that would have been sent, so a test can assert what the request contained. Failures on the live side of the seam are invisible to tests unless a Fixture replays them.

## Streams and the programs over them

### Stream
The server-sent event stream an API returns for one turn: Messages events for the Messages path, session events for a Session.

### Printer
A jq program that prints a Stream while it arrives, one string per event, with markers for tool use and nothing for events the user need not see. Everything a Printer emits passes through the safe filter.

### Fold
A jq program that reads a whole Stream at once and produces one value from it. The Messages fold reassembles the reply into the assistant turn; the session fold produces what the shell decides on after an idle: stop reason, usage, error, pending confirmations.
*Avoid:* reassembler, for the general concept; it names the Messages fold only.

### safe
The filter every string that came from the API passes through before it reaches the terminal. It keeps tab and newline and drops every other control character, so a reply cannot retitle the terminal, erase lines or write the clipboard.

## Chats and what they carry

### Chat
A conversation on the Messages path: one array of Turns, stored under the State directory, named by its start time. An empty Chat leaves no file. A Chat is listed, resumed by name or unique prefix, and continued line by line in the Chat loop.

### Turn
One user message and the assistant reply that answers it. A failed request rolls the Chat back to before the user's Turn, so a retry never sends two user Turns in a row.

### Chat loop
The interactive mode entered when there is no prompt: each line is a Turn, with commands to start a new Chat, print this one, and quit with the command that resumes it. One paste is one Turn.

### State directory
The per-user directory that holds Chats, their Containers and the Session stubs. It is private to the user, and a location other accounts can write is refused, because fixed file names there could be planted.

### Container
The code-execution sandbox a Chat keeps when it uses Skills, remembered beside the Chat so later Turns reuse the files from earlier ones. A rejected Container is forgotten so the next Turn starts a fresh one.

### Skill
An Anthropic skill, by id and optional version, run inside the Container for a Turn. Distinct from the agent skills in this repository's own tooling.

## Sessions and their resources

### Agent
A Managed Agents definition made once in the Console: model, system prompt, tools and the confirmation policy of each tool. ask-claude takes its id and never creates one.

### Environment
The sandbox definition a Session runs in, made once in the Console, with its allowed network hosts. ask-claude takes its id.

### Budget
The spend cap a Session is created with, at list price. Every Session gets one unless told otherwise, because session time bills per active second and a looping agent has no natural end. It can be raised later, and removed only for good.

### Tool confirmation
The pause before a tool runs: a Session's Agent wants to run a tool its policy marks ask, or the model asks for a Local tool. The user allows, denies, or allows for the rest of the run; with no terminal the answer is deny. An "always" answer lasts for the process that gave it, for that tool name, and never beyond. One gate serves both the Messages loop and Sessions.

### Custom tool
A tool the Agent declares that would run on the caller's machine. ask-claude runs none yet, so a request for one is answered with an error result rather than left hanging.

### Resource
Something attached to a Session at creation and made once in the Console: a Vault of credentials, a Memory store (optionally read-only), a mounted Repository, or an uploaded file. ask-claude passes ids and never creates them.

### Trace
A Session's page in the Console, printed as a URL.

## Local tools and plugins

### Local tool
A tool the model calls that runs on this machine, inside the current repository, behind a Tool confirmation: a bash command, an editor command, or a Local skill load. Distinct from a Custom tool, which a Session declares, and from the tools Anthropic runs in its own sandbox.

### Tool runner
The one function that executes a Local tool for the Messages loop, applies the confirmation gate first, and returns the result with an error flag. It bounds a command by a timeout and an output cap and confines an editor write to the repository.

### Local skill
A skill found on this machine in Claude Code's format, a directory with a SKILL.md, whose name and description go into the Skills index and whose body is loaded only when the model asks for it. Distinct from a Skill, which runs in Anthropic's Container.

### Skills index
The list of every discovered Local skill's name and description, placed in the system prompt when Local tools are on, so the model can choose one without the bodies in context.

### Plugin
A directory in Claude Code's plugin format, found through a marketplace file, that contributes Local skills and chat commands once enabled. Enablement is recorded in the State directory; a plugin's scripts see their own root through a substituted and exported path while one of its skills is active.

## Testing

### Oracle
The reference implementation whose behaviour defines correctness: originally the imported script, then the bash ask-claude once it reached parity, and the bash binary again while the Rust port is measured against it. A Golden is what the Oracle does, not a design choice.

### Golden
The expected output recorded by running the Oracle over a Fixture. A Golden changes only in a commit that says why the Oracle was wrong.

### Fixture
A recorded Stream, or a recorded request or response body, with every id and name redacted, tracked in the repository so the Unit suite runs offline. A Fixture that cannot be recorded, such as a refusal, is derived by hand from a recorded one.

### Redaction
The pass that replaces every API identifier and name in a Fixture before it is tracked. It matches values, never keys, and it is tested without the API.

### Unit suite
The bats tests that run the script over Fixtures through the Replay seam and compare against Goldens. It needs no key.

### Live contract
A hurl file that makes one cheap call against a real API and checks the shape of what comes back. Live contracts are the only tests that read the key, and none runs by default.

## The loop and its commits

### Plan row
One line of the plan: a commit to make, the files it touches, and the validation that proves it. Its status moves from not started, to tests written and red, to green.

### Test-then-feat pair
A test commit, red on its parent for exactly the tests it adds, and its child feat commit, which turns it green with no edit to the test. A test that has to change gets its own commit with the reason.

### TODO commit
An empty commit whose description is the instruction for a step not yet built. It holds a place in the graph so the plan is visible in history.

### Gate
The ordered list of commands that must pass before a version is cut, held in an empty verifier commit at the end of a milestone. Running it produces a Verified commit.

A Gate passes at zero failures. A test that cannot pass in some environment is excluded there by name or tag, so the exclusion is visible in the command; a tolerated count is a red baseline, and a red baseline hides new failures.

### Verified commit
An empty commit recording that a Gate passed: where it ran, when, and every command with its result.

### Version commit
The commit that bumps the version, adds the changelog section, and justifies its Effort from a diff of the usage spec.

## Versioning

### Effort
EffVer's measure of a release: how much work adopting it takes. Macro is a large effort, meso some, micro none. Before 1.0 the second number is macro and the third micro.

### Effort floor
The lowest Effort a release may claim, found by diffing the usage spec against the last version: a breaking interface change forbids micro.

### Usage spec
The declarative description of the command line, carried at the top of the script, from which help, parsing and shell completion are generated. Diffing two specs is how interface change is measured, for the Effort floor and for parity between the bash and Rust binaries.

### Parity
The point at which the Rust port replaces the bash script: every unit test green on both binaries, an empty usage spec diff, and the Live contracts green.

## Flagged ambiguities

- "provider" had been used for Venice — Venice is a Base URL under the OpenAI Provider, not a Provider.
- "session" had been used for a saved conversation — a saved conversation is a Chat; a Session is the Managed Agents transport.
- "over-bump" had meant raising the Effort to mark a milestone — Effort labels adoption cost only, so a milestone gets the Effort it earns.

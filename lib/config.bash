# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/config.bash: the checks every form runs before anything else, and the
# state directory's. Top-level code: sourced by ask-claude, it runs in place,
# as it did in the single file.
#
# reads:   usage_provider usage_base_url usage_mcp usage_max_tokens
#          ASK_CLAUDE_MCP_TOKEN ANTHROPIC_API_KEY; STATE (ask-claude)
# sets:    BASE_URL API HOST (read by common, messages, agent-api, agent-turn),
#          mcp_count mcp_vars h state_owner
# defines: need_key
# calls:   die (ask-claude)

# Providers. The bash version speaks the Messages API only; the OpenAI
# chat-completions shape (OpenAI, Venice at its own base URL) is the Rust
# port's (docs/plan.md, phase 3). Refused before any key check or request.
case "${usage_provider:-anthropic}" in
anthropic) ;;
openai) die "--provider openai is not implemented in the bash version, see docs/plan.md" ;;
*) die "unknown provider \"$usage_provider\" (anthropic|openai)" ;;
esac

# --base-url: the API root, https://api.anthropic.com unless given. Anything
# but https goes to a loopback address only, so the key never travels in
# clear over a network. The host is what curl will connect to, so the check
# is on the whole authority: "http://localhost:@example.com" starts like
# localhost, but curl reads "localhost:" as user info and connects to
# example.com. No API root carries user info, so any "@" is refused.
BASE_URL="${usage_base_url:-https://api.anthropic.com}"
case "$BASE_URL" in
*@*) die "--base-url must not carry user info (an @), got: $BASE_URL" ;;
https://*) ;;
*)
  [[ $BASE_URL =~ ^http://(localhost|127\.0\.0\.1|\[::1\])(:[0-9]+)?(/.*)?$ ]] ||
    die "--base-url must be https:// (or http:// on localhost), got: $BASE_URL"
  ;;
esac
# shellcheck disable=SC2034  # read by lib/messages.bash
API="${BASE_URL%/}/v1/messages"
# the event log records where requests went, never the path or a query
HOST="${BASE_URL#*://}"
HOST="${HOST%%/*}"

# --mcp: servers the model may call through the MCP connector; https only,
# like the docs require, and checked here so a typo is exit 2, not a 400.
mcp_count=0 mcp_vars=""
for v in ${usage_mcp:-}; do
  case "$v" in https://*) ;; *) die "--mcp takes https URLs, got: $v" ;; esac
  mcp_count=$((mcp_count + 1))
  # the variable lib/request.jq reads for this server's token: the host as
  # its `host` cuts it, upper-cased, every other character an underscore
  h=${v#https://}
  h=${h%%[/:]*}
  h=${h//[!A-Za-z0-9_]/_}
  mcp_vars+=" ASK_CLAUDE_MCP_TOKEN_${h^^}"
done
# A token is one server's credential: sent to several, each server's operator
# would receive the others'. Refuse rather than drop it in silence.
if [ "$mcp_count" -gt 1 ] && [ -n "${ASK_CLAUDE_MCP_TOKEN:-}" ]; then
  die "ASK_CLAUDE_MCP_TOKEN would go to all $mcp_count --mcp servers; unset it and give each server its own:$mcp_vars"
fi

# The key is checked where a request can happen (one shot, chat loop), so
# `list` works without one.
need_key() { [ -n "${ANTHROPIC_API_KEY:-}" ] || die "no key: export ANTHROPIC_API_KEY"; }
command -v jq >/dev/null || die "jq not found"
case "$usage_max_tokens" in
"" | *[!0-9]* | 0*) die "--max-tokens must be a positive integer, got: $usage_max_tokens" ;;
esac

# The script writes fixed names into $STATE (.tmp, .turn, .sse, .container,
# .stream.<pid>, .always.<pid>), and an .always.<pid> file allows every tool
# confirmation of that process. In a directory another account owns or can
# write, that account could plant those names. Refuse such a directory: a
# symlink (whose target is not ours to vouch for), one we cannot create or
# make private, one another account owns, one others can write.
mkdir -p "$STATE" || die "cannot create $STATE"
[ ! -L "$STATE" ] || die "$STATE is a symlink; keep the history in a private directory, not a link to one"
chmod 700 "$STATE" || die "cannot make $STATE private (chmod 700); keep the history in a directory you own"
# shellcheck disable=SC2012  # ls -ldn is portable to macOS; the path is ours, not user input
state_owner=$(ls -ldn "$STATE" | awk '{print $3}')
[ "$state_owner" = "$(id -u)" ] || die "$STATE is owned by uid $state_owner, not you; keep the history in a directory you own"
# shellcheck disable=SC2012
case "$(ls -ldn "$STATE" | cut -c6,9)" in
*w*) die "$STATE is writable by other accounts; keep the history in a private directory" ;;
esac

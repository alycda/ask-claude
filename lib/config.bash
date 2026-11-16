# shellcheck shell=bash
# shellcheck disable=SC2154  # usage_* come from the usage runtime, the rest from ask-claude and the modules sourced before this one
# lib/config.bash: the checks every form runs before anything else, and the
# state directory's. Top-level code: sourced by ask-claude, it runs in place,
# as it did in the single file.
#
# reads:   usage_provider usage_base_url usage_mcp usage_mcp_tool usage_github
#          usage_max_tokens ASK_CLAUDE_MCP_TOKEN ASK_CLAUDE_MCP_TOKEN_API_GITHUBCOPILOT_COM
#          ANTHROPIC_API_KEY ANTHROPIC_BASE_URL; STATE (ask-claude)
# sets:    BASE_URL API HOST (read by common, messages, agent-api, agent-turn),
#          usage_mcp usage_mcp_tool (unquoted, space-joined; --github adds
#          to the first; read by messages), USAGE_WORDS mcp_urls mcp_tools
#          GITHUB_MCP_URL base_url_from mcp_count mcp_vars mcp_hosts h t state_owner
# defines: need_key ascii_lower usage_words
# calls:   die (ask-claude), safe_text (common)

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
# Without the flag, usage takes ANTHROPIC_BASE_URL (env= in the spec), the
# official SDK's name, under the same gate; the provider check above keeps
# it to -p anthropic. usage does not say which of the two the value came
# from, so a value equal to the variable's counts as the variable's: they
# differ only when the flag repeats the variable, and then what the
# messages below say is still true. The variable is not typed at this
# prompt and may carry terminal codes, so what they print of the root goes
# through safe.
BASE_URL="${usage_base_url:-https://api.anthropic.com}"
base_url_from=--base-url
[ -z "${ANTHROPIC_BASE_URL:-}" ] || [ "$BASE_URL" != "$ANTHROPIC_BASE_URL" ] || base_url_from=ANTHROPIC_BASE_URL
case "$BASE_URL" in
*@*) die "$base_url_from must not carry user info (an @), got: $(safe_text "$BASE_URL")" ;;
https://*) ;;
*)
  [[ $BASE_URL =~ ^http://(localhost|127\.0\.0\.1|\[::1\])(:[0-9]+)?(/.*)?$ ]] ||
    die "$base_url_from must be https:// (or http:// on localhost), got: $(safe_text "$BASE_URL")"
  ;;
esac
# shellcheck disable=SC2034  # read by lib/messages.bash
API="${BASE_URL%/}/v1/messages"
# the event log records where requests went, never the path or a query
HOST="${BASE_URL#*://}"
HOST="${HOST%%/*}"
# The variable is often exported for other tools, and the key goes where it
# points: a root from it that is not the default is said once, on stderr.
if [ "$base_url_from" = ANTHROPIC_BASE_URL ] && [ "${BASE_URL%/}" != https://api.anthropic.com ]; then
  echo "ask-claude: ANTHROPIC_BASE_URL is set; requests go to $(safe_text "$HOST")" >&2
fi

# usage_words <joined>: the values of a repeatable flag, into the array
# USAGE_WORDS. usage joins them with spaces and quotes each the way the
# shell-words crate does: as it is when every character is in its safe set,
# else in single quotes with each ' written '\'' ('' for an empty value). A
# URL with a query string (?, &, =) arrives quoted. This undoes that quoting
# and nothing else. There is no eval, so nothing in a value expands: not
# $(...), not a glob, and not the {a,b} the safe set leaves bare. Returns 1
# on a string that is not in that form.
usage_words() {
  local s=$1 w re_quoted="^'([^']*)'(.*)$" re_bare="^([^ '\\]+)(.*)$"
  USAGE_WORDS=()
  while [ -n "$s" ]; do
    w=""
    while [ -n "$s" ] && [ "${s:0:1}" != " " ]; do
      if [[ $s =~ $re_quoted ]] || [[ $s =~ $re_bare ]]; then
        w+=${BASH_REMATCH[1]} s=${BASH_REMATCH[2]}
      elif [ "${s:0:2}" = "\\'" ]; then
        w+="'" s=${s:2}
      else
        return 1
      fi
    done
    USAGE_WORDS+=("$w")
    s=${s# }
  done
}

# --mcp and --mcp-tool, as lists. The checks below run on these, and both
# flags are written back joined with single spaces, unquoted, for
# lib/request.jq to split. That is safe because a URL that passes the checks
# has no whitespace, and an --mcp-tool entry has no space in its character set.
usage_words "${usage_mcp:-}" || die "cannot read the --mcp values usage passed: ${usage_mcp:-}"
mcp_urls=("${USAGE_WORDS[@]}")
usage_words "${usage_mcp_tool:-}" || die "cannot read the --mcp-tool values usage passed: ${usage_mcp_tool:-}"
mcp_tools=("${USAGE_WORDS[@]}")

# --github: shorthand for the long form, --mcp GITHUB_MCP_URL: GitHub's MCP
# server with its repos toolset, read-only. It adds that URL to the --mcp
# list here, before every --mcp check, so from then on it is one --mcp server
# like any other (its token variable, --mcp-tool, the duplicate-host check).
# GitHub answers 401 to a request without a token on every path, /readonly
# included, so the shorthand requires the server's own variable: exit 2 here,
# not a failed request. ASK_CLAUDE_MCP_TOKEN does not count: the shorthand
# names one variable, and it is the one it reads.
readonly GITHUB_MCP_URL=https://api.githubcopilot.com/mcp/x/repos/readonly
if [ "${usage_github:-}" = true ]; then
  [ -n "${ASK_CLAUDE_MCP_TOKEN_API_GITHUBCOPILOT_COM:-}" ] ||
    die "--github needs ASK_CLAUDE_MCP_TOKEN_API_GITHUBCOPILOT_COM, a GitHub token (a fine-grained token with public repositories read-only is enough)"
  mcp_urls+=("$GITHUB_MCP_URL")
fi

# --mcp: servers the model may call through the MCP connector; https only,
# like the docs require, and checked here so a typo is exit 2, not a 400. A
# URL with whitespace or a control character is refused too: it is no URL,
# and the list goes to request.jq space-joined.
# The host ends at the first / : ? or #, the authority's end (a port's colon
# included), exactly where lib/request.jq's `host` cuts it.
# One server per host: lib/request.jq names a server after its host and
# reads its token from the host's variable, so two URLs on one host (two
# paths, a port, the host in another case, or --github beside an --mcp on
# api.githubcopilot.com) would go out under one name, which the API takes
# once, and share one token. The variable is the key: it folds everything
# the name folds, and case too.
mcp_count=0 mcp_vars=""
for v in "${mcp_urls[@]}"; do
  case "$v" in
  *[[:space:][:cntrl:]]*) die "--mcp takes https URLs, got one with whitespace or a control character: ${v@Q}" ;;
  https://*) ;;
  *) die "--mcp takes https URLs, got: $v" ;;
  esac
  mcp_count=$((mcp_count + 1))
  # the variable lib/request.jq reads for this server's token: the host as
  # its `host` cuts it, upper-cased, every other character an underscore
  h=${v#https://}
  h=${h%%[/:?#]*}
  h=${h//[!A-Za-z0-9_]/_}
  case "$mcp_vars " in
  *" ASK_CLAUDE_MCP_TOKEN_${h^^} "*)
    die "--mcp $v is on the host of another --mcp server (or of --github): the API takes one server per name, and request.jq names a server after its host; give one URL per host" ;;
  esac
  mcp_vars+=" ASK_CLAUDE_MCP_TOKEN_${h^^}"
done
usage_mcp="${mcp_urls[*]}"
# --mcp-tool host:tool: that server offers only the tools named (lib/request.jq
# turns its toolset into an allowlist). An entry whose host is no --mcp server
# would match nothing, and a rule that matches nothing reads like a rule while
# every tool stays on: exit 2 instead. The tool part is an MCP tool name,
# letters, digits and _ . - (the MCP spec's set), so it cannot carry a colon.
# Hosts fold case ASCII only, exactly as request.jq's ascii_downcase does:
# bash ${h,,} follows the locale and folds characters jq does not (U+212A,
# the Kelvin sign, to k), so an entry could pass here, match nothing in the
# request, and leave that server with every tool on.
# shellcheck disable=SC2018,SC2019  # ASCII only, on purpose: the classes would follow the locale
ascii_lower() { LC_ALL=C tr 'A-Z' 'a-z' <<<"$1"; }
mcp_hosts=" "
for v in "${mcp_urls[@]}"; do
  h=${v#https://}
  h=${h%%[/:?#]*}
  mcp_hosts+="$(ascii_lower "$h") "
done
for t in "${mcp_tools[@]}"; do
  [[ $t =~ ^[A-Za-z0-9.-]+:[A-Za-z0-9_.-]{1,128}$ ]] ||
    die "--mcp-tool takes <host>:<tool>, e.g. mcp.example.com:search, got: $t"
  h=${t%%:*}
  case "$mcp_hosts" in
  *" $(ascii_lower "$h") "*) ;;
  *) die "--mcp-tool $t names no --mcp server (hosts:${mcp_hosts% }); it would allow every tool, not only $h's ${t#*:}" ;;
  esac
done
usage_mcp_tool="${mcp_tools[*]}"
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

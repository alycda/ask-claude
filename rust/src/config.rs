//! The checks every form runs before anything else (`lib/config.bash`), in
//! its order: provider, `--base-url` (or `ANTHROPIC_BASE_URL`), `--mcp`,
//! `--mcp-tool`, the shared MCP token, `--max-tokens`. Each refusal is the
//! script's message, exit 2, before any key check, request or history file.

use crate::cli::RequestFlags;

/// What the checks hand on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// the API root, `https://api.anthropic.com` unless the flag or
    /// `ANTHROPIC_BASE_URL` gives one
    pub base_url: String,
    pub max_tokens: u64,
}

/// The checks, or the message (without the `ask-claude: ` prefix) of the
/// first that fails.
pub fn check(r: &RequestFlags) -> Result<Config, String> {
    match r.provider.as_str() {
        "anthropic" => {}
        // R7 (U6) builds it; until then the Rust version refuses it the way
        // the bash version does. clap fills `base_url` from
        // ANTHROPIC_BASE_URL whatever the provider: R7 must drop a value
        // that came from it (ArgMatches::value_source) and keep
        // OPENAI_BASE_URL for this provider (KTD17).
        "openai" => {
            return Err(
                "--provider openai is not implemented in the Rust version yet, see docs/plan.md"
                    .into(),
            );
        }
        p => return Err(format!("unknown provider \"{p}\" (anthropic|openai)")),
    }

    // `${usage_base_url:-...}`: an empty value, from the flag or the
    // variable, is no value
    let base_url = r
        .base_url
        .clone()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.into());
    // a value equal to ANTHROPIC_BASE_URL's counts as the variable's, as the
    // script counts it (usage does not say which supplied it): they differ
    // only when the flag repeats the variable
    let from_env =
        std::env::var("ANTHROPIC_BASE_URL").is_ok_and(|v| !v.is_empty() && v == base_url);
    let source = if from_env {
        "ANTHROPIC_BASE_URL"
    } else {
        "--base-url"
    };
    base_url_gate(&base_url, source)?;
    // the variable is often exported for other tools, and the key goes where
    // it points: a root from it that is not the default is said once, where
    // the script says it, before the checks that follow
    let root = base_url.strip_suffix('/').unwrap_or(&base_url);
    if from_env && root != DEFAULT_BASE_URL {
        let rest = base_url
            .split_once("://")
            .map_or(&*base_url, |(_, rest)| rest);
        let host = &rest[..rest.find('/').unwrap_or(rest.len())];
        eprintln!("ask-claude: ANTHROPIC_BASE_URL is set; requests go to {host}");
    }

    // usage joins a repeated flag's values with spaces, and the script
    // splits them again
    let mcp: Vec<&str> = r.mcp.iter().flat_map(|m| m.split_whitespace()).collect();
    let mut vars = String::new();
    for v in &mcp {
        let Some(rest) = v.strip_prefix("https://") else {
            return Err(format!("--mcp takes https URLs, got: {v}"));
        };
        // the variable lib/request.jq reads for this server's token
        let h: String = host(rest)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect();
        vars.push_str(&format!(" ASK_CLAUDE_MCP_TOKEN_{h}"));
    }

    // hosts fold case ASCII only, exactly as request.jq's ascii_downcase
    // does, so an entry cannot pass here and match nothing in the request
    let hosts: Vec<String> = mcp
        .iter()
        .map(|v| host(v.strip_prefix("https://").unwrap_or(v)).to_ascii_lowercase())
        .collect();
    for t in r.mcp_tool.iter().flat_map(|m| m.split_whitespace()) {
        let Some((h, tool)) = t.split_once(':').filter(|(h, tool)| {
            !h.is_empty()
                && h.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
                && (1..=128).contains(&tool.len())
                && tool
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        }) else {
            return Err(format!(
                "--mcp-tool takes <host>:<tool>, e.g. mcp.example.com:search, got: {t}"
            ));
        };
        if !hosts.contains(&h.to_ascii_lowercase()) {
            let listed: String = hosts.iter().map(|h| format!(" {h}")).collect();
            return Err(format!(
                "--mcp-tool {t} names no --mcp server (hosts:{listed}); it would allow every tool, not only {h}'s {tool}"
            ));
        }
    }

    // a token is one server's credential: sent to several, each server's
    // operator would receive the others'
    let shared = std::env::var_os("ASK_CLAUDE_MCP_TOKEN").is_some_and(|t| !t.is_empty());
    if mcp.len() > 1 && shared {
        return Err(format!(
            "ASK_CLAUDE_MCP_TOKEN would go to all {} --mcp servers; unset it and give each server its own:{vars}",
            mcp.len()
        ));
    }

    let n = &r.max_tokens;
    let max_tokens = n
        .parse::<u64>()
        .ok()
        .filter(|_| !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| format!("--max-tokens must be a positive integer, got: {n}"))?;

    Ok(Config {
        base_url,
        max_tokens,
    })
}

/// The API root without `--base-url` or `ANTHROPIC_BASE_URL`.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// `--base-url`: https, or plain http to a loopback host only, so the key
/// never travels in clear over a network. The check is on the whole
/// authority: any `@` (user info) is refused, since `http://localhost:@x`
/// starts like localhost and connects to x. `source` names where the value
/// came from, the flag or `ANTHROPIC_BASE_URL`.
fn base_url_gate(url: &str, source: &str) -> Result<(), String> {
    if url.contains('@') {
        return Err(format!(
            "{source} must not carry user info (an @), got: {url}"
        ));
    }
    if url.starts_with("https://") {
        return Ok(());
    }
    // ^http://(localhost|127\.0\.0\.1|\[::1\])(:[0-9]+)?(/.*)?$
    let loopback = url.strip_prefix("http://").is_some_and(|rest| {
        ["localhost", "127.0.0.1", "[::1]"].iter().any(|h| {
            rest.strip_prefix(h).is_some_and(|rest| {
                let rest = match rest.strip_prefix(':') {
                    Some(port) => {
                        let digits = port.bytes().take_while(u8::is_ascii_digit).count();
                        if digits == 0 {
                            return false;
                        }
                        &port[digits..]
                    }
                    None => rest,
                };
                rest.is_empty() || rest.starts_with('/')
            })
        })
    });
    if loopback {
        Ok(())
    } else {
        Err(format!(
            "{source} must be https:// (or http:// on localhost), got: {url}"
        ))
    }
}

/// An authority's host the way the script cuts it: up to the first `/` or
/// `:` (`${h%%[/:]*}`), user info included.
fn host(rest: &str) -> &str {
    &rest[..rest.find(['/', ':']).unwrap_or(rest.len())]
}

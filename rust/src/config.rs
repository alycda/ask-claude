//! The checks every form runs before anything else (`lib/config.bash`), in
//! its order: provider, `--base-url`, `--mcp`, `--mcp-tool`, the shared MCP
//! token, `--max-tokens`. Each refusal is the script's message, exit 2,
//! before any key check, request or history file.

use crate::cli::RequestFlags;

/// What the checks hand on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// the API root, `https://api.anthropic.com` unless given
    pub base_url: String,
    pub max_tokens: u64,
}

/// The checks, or the message (without the `ask-claude: ` prefix) of the
/// first that fails.
pub fn check(r: &RequestFlags) -> Result<Config, String> {
    match r.provider.as_str() {
        "anthropic" => {}
        // R7 (U6) builds it; until then the Rust version refuses it the way
        // the bash version does
        "openai" => {
            return Err(
                "--provider openai is not implemented in the Rust version yet, see docs/plan.md"
                    .into(),
            );
        }
        p => return Err(format!("unknown provider \"{p}\" (anthropic|openai)")),
    }

    let base_url = r
        .base_url
        .clone()
        .unwrap_or_else(|| "https://api.anthropic.com".into());
    base_url_gate(&base_url)?;

    // usage joins a repeated flag's values with spaces, and the script
    // splits them again
    let mcp: Vec<&str> = r.mcp.iter().flat_map(|m| words(m)).collect();
    let mut vars = String::new();
    for v in &mcp {
        if !v.starts_with("https://") {
            return Err(format!("--mcp takes https URLs, got: {v}"));
        }
        // the variable the request reads for this server's token
        vars.push_str(&format!(" {}", crate::request::token_variable(v)));
    }

    // hosts fold case ASCII only, exactly as request.jq's ascii_downcase
    // does, so an entry cannot pass here and match nothing in the request
    let hosts: Vec<String> = mcp.iter().map(|v| crate::request::hostname(v)).collect();
    for t in r.mcp_tool.iter().flat_map(|m| words(m)) {
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

/// `--base-url`: https, or plain http to a loopback host only, so the key
/// never travels in clear over a network. The check is on the whole
/// authority: any `@` (user info) is refused, since `http://localhost:@x`
/// starts like localhost and connects to x.
fn base_url_gate(url: &str) -> Result<(), String> {
    if url.contains('@') {
        return Err(format!(
            "--base-url must not carry user info (an @), got: {url}"
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
            "--base-url must be https:// (or http:// on localhost), got: {url}"
        ))
    }
}

/// `for v in ${usage_mcp:-}`: a value's words as bash splits them on the
/// default IFS (space, tab, newline) and no other whitespace, so the check
/// counts and reads the entries the script counts and reads.
fn words(value: &str) -> impl Iterator<Item = &str> {
    value.split([' ', '\t', '\n']).filter(|w| !w.is_empty())
}

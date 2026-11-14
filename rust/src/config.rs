//! The checks every form runs before anything else (`lib/config.bash`), in
//! its order: provider, `--base-url` (or `ANTHROPIC_BASE_URL`), `--mcp`,
//! `--mcp-tool`, the shared MCP token, `--max-tokens`. Each refusal is the
//! script's message, exit 2, before any key check, request or history file.
//! `-p openai`, which the script refuses, takes its root and model from its
//! own variables and refuses the Messages API's server tools.

use crate::cli::RequestFlags;

/// What the checks hand on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub provider: Provider,
    /// `-m`; with `-p openai` and no `-m` on the command line,
    /// `OPENAI_MODEL` when it is set
    pub model: String,
    pub max_tokens: u64,
}

/// Which API shape a request takes, and the Base URL it goes to
/// (CONCEPTS.md). A vendor that serves one of these shapes is a base URL,
/// not a provider: Venice is `OpenAiCompatible` at its own root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    /// The Messages API, `-p anthropic`: `https://api.anthropic.com`
    /// unless `--base-url` or `ANTHROPIC_BASE_URL` names a root.
    Anthropic { base_url: String },
    /// The chat-completions shape, `-p openai` ([`crate::openai`]):
    /// `https://api.openai.com/v1` unless `--base-url` or `OPENAI_BASE_URL`
    /// names a root.
    OpenAiCompatible { base_url: String },
}

impl Provider {
    /// The checked API root, as given (a trailing slash included).
    pub fn base_url(&self) -> &str {
        match self {
            Provider::Anthropic { base_url } | Provider::OpenAiCompatible { base_url } => base_url,
        }
    }

    /// The variable the key comes from.
    pub fn key_variable(&self) -> &'static str {
        match self {
            Provider::Anthropic { .. } => "ANTHROPIC_API_KEY",
            Provider::OpenAiCompatible { .. } => "OPENAI_API_KEY",
        }
    }
}

/// The checks, or the message (without the `ask-claude: ` prefix) of the
/// first that fails.
pub fn check(r: &RequestFlags) -> Result<Config, String> {
    let (provider, model) = match r.provider.as_str() {
        "anthropic" => (
            Provider::Anthropic {
                base_url: anthropic_base_url(r)?,
            },
            r.model.clone(),
        ),
        "openai" => {
            let provider = Provider::OpenAiCompatible {
                base_url: openai_base_url(r)?,
            };
            anthropic_only(r)?;
            (provider, openai_model(r))
        }
        p => return Err(format!("unknown provider \"{p}\" (anthropic|openai)")),
    };

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
        provider,
        model,
        max_tokens,
    })
}

/// The API root without `--base-url` or `ANTHROPIC_BASE_URL`.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// The API root of `-p openai` without `--base-url` or `OPENAI_BASE_URL`
/// (docs/plan.md, phase 3).
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// `-p anthropic`'s root: `--base-url`, or `ANTHROPIC_BASE_URL` (KTD17),
/// through the gate, announced on stderr when the variable moves it.
fn anthropic_base_url(r: &RequestFlags) -> Result<String, String> {
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
    Ok(base_url)
}

/// `-p openai`'s root: `--base-url` from the command line, else
/// `OPENAI_BASE_URL`, else OpenAI's, through the same gate. clap fills
/// `--base-url` from `ANTHROPIC_BASE_URL` whatever the provider; that value
/// is dropped here (KTD17), told apart from the flag by where clap found
/// it, so it is neither sent the OpenAI key nor gated nor announced.
fn openai_base_url(r: &RequestFlags) -> Result<String, String> {
    let flag = r
        .base_url
        .clone()
        .filter(|u| r.base_url_given && !u.is_empty());
    let (base_url, source) = match flag {
        Some(u) => (u, "--base-url"),
        None => match std::env::var("OPENAI_BASE_URL") {
            Ok(u) if !u.is_empty() => (u, "OPENAI_BASE_URL"),
            _ => (DEFAULT_OPENAI_BASE_URL.to_string(), "--base-url"),
        },
    };
    base_url_gate(&base_url, source)?;
    Ok(base_url)
}

/// `-p openai`'s model: `-m` from the command line, else `OPENAI_MODEL`
/// (the variable `just test-openai` reads), else `-m`'s default, which an
/// OpenAI-compatible API answers with its own error.
fn openai_model(r: &RequestFlags) -> String {
    match std::env::var("OPENAI_MODEL") {
        Ok(m) if !r.model_given && !m.is_empty() => m,
        _ => r.model.clone(),
    }
}

/// The flags that name the Messages API's server tools and betas, which
/// the chat-completions shape has no field for: refused, so a reply never
/// looks searched or skilled when it was not.
fn anthropic_only(r: &RequestFlags) -> Result<(), String> {
    let given = [
        ("--web", r.web),
        ("--mcp", !r.mcp.is_empty()),
        ("--mcp-tool", !r.mcp_tool.is_empty()),
        ("-s", r.skills.as_deref().is_some_and(|s| !s.is_empty())),
        (
            "--advisor",
            r.advisor.as_deref().is_some_and(|a| !a.is_empty()),
        ),
    ];
    match given.iter().find(|(_, on)| *on) {
        Some((flag, _)) => Err(format!("{flag} works with -p anthropic only")),
        None => Ok(()),
    }
}

/// `--base-url`: https, or plain http to a loopback host only, so the key
/// never travels in clear over a network. The check is on the whole
/// authority: any `@` (user info) is refused, since `http://localhost:@x`
/// starts like localhost and connects to x. `source` names where the value
/// came from, the flag or the provider's variable.
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

use serde_json::Value;

/// What one event prints: text deltas, `(thinking) `, `(web_search) `,
/// `(running code) `, a newline at `message_stop`, "" otherwise; every
/// control character but tab and newline removed (`safe`).
pub fn line(event: &Value) -> String {
    use crate::{or, text};
    let q = Value::from("?");
    let out = match event["type"].as_str().unwrap_or("") {
        "content_block_start" => {
            let b = &event["content_block"];
            let name = b["name"].as_str();
            match b["type"].as_str().unwrap_or("") {
                "thinking" => "(thinking) ".to_string(),
                "server_tool_use" => match name {
                    Some(n @ ("web_search" | "web_fetch")) => format!("({n}) "),
                    Some("advisor") => "(advisor) ".to_string(),
                    _ => "(running code) ".to_string(),
                },
                "advisor_tool_result" if b["content"]["type"] == "advisor_tool_result_error" => {
                    format!(
                        "(advisor error: {}) ",
                        text(or(&b["content"]["error_code"], &q))
                    )
                }
                "mcp_tool_use" => format!("(mcp {}) ", text(or(&b["server_name"], &q))),
                "bash_code_execution_tool_result" => {
                    let rc = &b["content"]["return_code"];
                    let rc = if rc.is_number() {
                        rc.to_string()
                    } else {
                        "?".to_string()
                    };
                    format!("(result rc={rc}) ")
                }
                _ => String::new(),
            }
        }
        "content_block_delta" if event["delta"]["type"] == "text_delta" => {
            text(&event["delta"]["text"])
        }
        "message_stop" => "\n".to_string(),
        _ => String::new(),
    };
    safe(&out)
}

/// What one event prints on a stream that gets colour (docs/plan.md, phase
/// 6): `line`, with each marker inside one span. A marker is dim; a result
/// whose return code is not 0 and an advisor error are red. The reply's
/// text is never coloured. A span opens and closes inside the one string an
/// event prints, so nothing can leave the terminal coloured, and the
/// sequences are added after `safe`, which still removes an escape that
/// came from the API.
pub fn coloured(event: &Value) -> String {
    let _ = event;
    todo!("K6: feat: the stream printer colours markers")
}

/// The `safe` of every printer (`def safe` in `lib/stream.jq`, the `SAFE`
/// prefix in `lib/common.bash`): keep tab and newline, drop every other C0
/// and C1 control character, so a reply cannot drive the terminal, and the
/// nine bidirectional formatting characters U+202A to U+202E and U+2066 to
/// U+2069, so it cannot reorder what the user reads. The marks U+200E,
/// U+200F and U+061C stay, as ordinary right-to-left text.
pub fn safe(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            let c = c as u32;
            c == 9
                || c == 10
                || (c >= 32
                    && c != 127
                    && !(128..=159).contains(&c)
                    && !(0x202a..=0x202e).contains(&c)
                    && !(0x2066..=0x2069).contains(&c))
        })
        .collect()
}

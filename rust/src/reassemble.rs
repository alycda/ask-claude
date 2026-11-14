use serde_json::Value;

/// The events of one reply, in order, into the message the history
/// keeps: `{model, stop_reason, stop_details, usage, container, error,
/// content}` in that key order, with `content` the blocks in index order
/// and tool inputs parsed from their `input_json_delta` partials
/// (`lib/reassemble.jq`). An `Err` is a reply jq cannot fold, which `call`
/// in `lib/messages.bash` reports as an API error. A tool input that is not
/// JSON at its `content_block_stop` is one whatever the stop reason, since
/// jq's `fromjson` fails there before the stop reason is read; U3 of the
/// rust-parity plan said `max_tokens` only, and the oracle wins.
///
/// Blocks are keyed as jq keys them, `$e.index|tostring`, so a string
/// index names the block its number does, and they are ordered by
/// `sort_by(.key|tonumber)`: a key that is no number (a missing or null
/// index, say) fails the fold as `tonumber` does.
pub fn message(events: &[Value]) -> Result<Value, serde_json::Error> {
    use serde::de::Error as _;
    use serde_json::{Error, Map, json};

    let mut blocks = Map::new();
    let mut model = Value::Null;
    let mut stop_reason = Value::Null;
    let mut stop_details = Value::Null;
    let mut usage = Value::Object(Map::new());
    let mut container = Value::Null;
    let mut error = Value::Null;

    /// jq's `.k += $s` on a string field that may be absent.
    fn append(block: &mut Value, key: &str, s: &Value) {
        let s = s.as_str().unwrap_or("");
        match block.get_mut(key) {
            Some(Value::String(cur)) => cur.push_str(s),
            _ => block[key] = Value::String(s.to_string()),
        }
    }

    for e in events {
        let index = match &e["index"] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        match e["type"].as_str().unwrap_or("") {
            "message_start" => {
                model = e["message"]["model"].clone();
                usage = e["message"]["usage"].clone();
                container = e["message"]["container"].clone();
            }
            "content_block_start" => {
                let mut b = e["content_block"].clone();
                if !(b.is_object() || b.is_null()) {
                    // jq's `.type` on it: "Cannot index string with string"
                    return Err(Error::custom("a content block that is not an object"));
                }
                if matches!(
                    b["type"].as_str(),
                    Some("tool_use" | "server_tool_use" | "mcp_tool_use")
                ) {
                    b["partial"] = json!("");
                }
                blocks.insert(index, b);
            }
            "content_block_delta" => {
                let d = &e["delta"];
                // a delta of another type is jq's `else .`: no block made
                let (key, s) = match d["type"].as_str().unwrap_or("") {
                    "text_delta" => ("text", &d["text"]),
                    "thinking_delta" => ("thinking", &d["thinking"]),
                    "signature_delta" => ("signature", &d["signature"]),
                    "input_json_delta" => ("partial", &d["partial_json"]),
                    _ => continue,
                };
                let b = blocks.entry(index).or_insert_with(|| json!({}));
                if key == "signature" {
                    b[key] = s.clone();
                } else {
                    append(b, key, s);
                }
            }
            "content_block_stop" => {
                if let Some(Value::Object(b)) = blocks.get_mut(&index)
                    && let Some(partial) = b.remove("partial")
                {
                    let p = partial.as_str().unwrap_or("");
                    let input = if p.is_empty() {
                        json!({})
                    } else {
                        fromjson(p)?
                    };
                    b.insert("input".into(), input);
                }
            }
            "message_delta" => {
                stop_reason = e["delta"]["stop_reason"].clone();
                stop_details = e["delta"]["stop_details"].clone();
                if let Some(add) = e["usage"].as_object() {
                    if !usage.is_object() {
                        usage = Value::Object(Map::new());
                    }
                    if let Some(u) = usage.as_object_mut() {
                        for (k, v) in add {
                            u.insert(k.clone(), v.clone());
                        }
                    }
                }
                if !e["delta"]["container"].is_null() {
                    container = e["delta"]["container"].clone();
                }
            }
            "error" => error = e["error"].clone(),
            _ => {}
        }
    }

    // FINISH: a stable sort, so keys of one number keep the order first seen
    let mut content = Vec::with_capacity(blocks.len());
    for (key, b) in blocks {
        let n: f64 = key
            .parse()
            .map_err(|_| Error::custom(format!("string ({key:?}) cannot be parsed as a number")))?;
        content.push((n, b));
    }
    content.sort_by(|a, b| a.0.total_cmp(&b.0));

    Ok(json!({
        "model": model,
        "stop_reason": stop_reason,
        "stop_details": stop_details,
        "usage": usage,
        "container": container,
        "error": error,
        "content": content.into_iter().map(|(_, b)| b).collect::<Vec<_>>(),
    }))
}

/// jq's `fromjson` on a tool input's partial: serde_json's parse, except
/// that a lone low surrogate escape is U+FFFD, as jq 1.8 reads it, where
/// serde_json refuses the text. A high surrogate with no low one after it
/// is refused by both. (jq 1.8.2 also reads more than JSON: `nan` in any
/// case or sign as null, `infinity` and `inf` as the largest double, a
/// number past f64's range as written, and arrays and objects nested up
/// to 10000 levels where serde_json stops at 128. Taking on the depth would
/// need that much recursion in every later walk of the value, more stack
/// than the main thread has, and the API sends none of that; serde_json
/// refuses it, and that one difference is left alone.)
pub(crate) fn fromjson(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text).or_else(|e| match lone_lows_replaced(text) {
        Some(fixed) => serde_json::from_str(&fixed),
        None => Err(e),
    })
}

/// `text` with each `\uDC00`..`\uDFFF` escape that does not end a
/// surrogate pair spelled `�`, or None when it has none. Escapes are
/// read pairwise from each backslash, so `\\udc00` is an escaped backslash
/// and the letters after it, and stays.
fn lone_lows_replaced(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let unit = |at: usize| {
        bytes
            .get(at..at + 6)
            .filter(|e| e.starts_with(b"\\u"))
            .and_then(|e| crate::sse::hex4(&e[2..]))
    };
    let mut out = String::new();
    let mut copied = 0;
    let mut i = 0;
    while let Some(at) = bytes
        .get(i..)
        .and_then(|rest| rest.iter().position(|&b| b == b'\\'))
        .map(|p| p + i)
    {
        i = match unit(at) {
            Some(0xD800..=0xDBFF) if matches!(unit(at + 6), Some(0xDC00..=0xDFFF)) => at + 12,
            Some(0xDC00..=0xDFFF) => {
                out.push_str(&text[copied..at]);
                out.push_str("\\uFFFD");
                copied = at + 6;
                at + 6
            }
            _ => at + 2,
        };
    }
    (copied > 0).then(|| out + &text[copied..])
}

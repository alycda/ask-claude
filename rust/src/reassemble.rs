use serde_json::Value;

/// The events of one reply, in order, into the message the history
/// keeps: `{model, stop_reason, stop_details, usage, container, error,
/// content}` in that key order, with `content` the blocks in index order
/// and tool inputs parsed from their `input_json_delta` partials.
pub fn message(events: &[Value]) -> Value {
    use serde_json::{Map, json};
    use std::collections::BTreeMap;

    let mut blocks: BTreeMap<u64, Value> = BTreeMap::new();
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
        let index = e["index"].as_u64().unwrap_or(0);
        match e["type"].as_str().unwrap_or("") {
            "message_start" => {
                model = e["message"]["model"].clone();
                usage = e["message"]["usage"].clone();
                container = e["message"]["container"].clone();
            }
            "content_block_start" => {
                let mut b = e["content_block"].clone();
                if matches!(
                    b["type"].as_str(),
                    Some("tool_use" | "server_tool_use" | "mcp_tool_use")
                ) {
                    b["partial"] = json!("");
                }
                blocks.insert(index, b);
            }
            "content_block_delta" => {
                let b = blocks.entry(index).or_insert_with(|| json!({}));
                let d = &e["delta"];
                match d["type"].as_str().unwrap_or("") {
                    "text_delta" => append(b, "text", &d["text"]),
                    "thinking_delta" => append(b, "thinking", &d["thinking"]),
                    "signature_delta" => b["signature"] = d["signature"].clone(),
                    "input_json_delta" => append(b, "partial", &d["partial_json"]),
                    _ => {}
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
                        serde_json::from_str(p).unwrap_or(Value::Null)
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
                    let u = usage.as_object_mut().unwrap();
                    for (k, v) in add {
                        u.insert(k.clone(), v.clone());
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

    json!({
        "model": model,
        "stop_reason": stop_reason,
        "stop_details": stop_details,
        "usage": usage,
        "container": container,
        "error": error,
        "content": blocks.into_values().collect::<Vec<_>>(),
    })
}

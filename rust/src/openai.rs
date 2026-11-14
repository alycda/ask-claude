//! `-p openai`: the chat-completions shape that OpenAI and other vendors
//! serve (docs/plan.md, phase 3; `tests/providers/openai-compatible.hurl`
//! pins it). The script has no twin: bash refuses the provider. Rather than
//! a second printer and fold, [`OpenAiCompatible`] sits around the
//! transport and translates both ways: the Messages body the request
//! builder made becomes a chat-completions body on the way out, and the
//! streamed `chat.completion.chunk`s come back as the Messages events they
//! amount to, so `lib/stream.jq`'s printer, `lib/reassemble.jq`'s fold, the
//! usage line, the chat file and the event log are the Messages path's.
//!
//! What the shape has no field for is not sent: the effort, the cache
//! marker, the fallbacks. The server tools are refused before this
//! (`config::check`). A history's blocks go back as their text only.

use std::io::{self, BufRead, BufReader, Read};

use serde_json::{Map, Value, json};

use crate::transport::{Reply, Transport};

/// The chat-completions body for a Messages body `request::body` made: the
/// model, `max_tokens` as `max_completion_tokens`, a stream with its usage
/// asked for, the system prompt as the first message, and each turn's
/// content as text, a block array as its text blocks joined. Keys in the
/// hurl contract's order.
pub fn body(messages_body: &Value) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = messages_body["system"].as_str() {
        messages.push(json!({"role": "system", "content": system}));
    }
    for turn in messages_body["messages"].as_array().into_iter().flatten() {
        let content = match &turn["content"] {
            Value::String(s) => s.clone(),
            Value::Array(blocks) => blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect(),
            _ => String::new(),
        };
        messages.push(json!({"role": turn["role"], "content": content}));
    }
    json!({
        "model": messages_body["model"],
        "stream": true,
        "max_completion_tokens": messages_body["max_tokens"],
        "stream_options": {"include_usage": true},
        "messages": messages,
    })
}

/// Another transport (the API or the replay seam, inside the test logs)
/// spoken to in the chat-completions shape: the request log holds the body
/// actually sent, and the replay seam serves recorded chunks.
pub struct OpenAiCompatible {
    inner: Box<dyn Transport + Send>,
}

impl OpenAiCompatible {
    pub fn new(inner: Box<dyn Transport + Send>) -> Self {
        Self { inner }
    }
}

impl Transport for OpenAiCompatible {
    /// The body translated, no `anthropic-beta` value (there is no such
    /// header here), and the reply read through [`Translated`].
    fn send(&mut self, body: &str, _betas: &str) -> io::Result<Reply> {
        let messages_body: Value = serde_json::from_str(body).map_err(io::Error::other)?;
        let reply = self
            .inner
            .send(&self::body(&messages_body).to_string(), "")?;
        Ok(Box::new(Translated {
            inner: BufReader::new(reply),
            chunks: Chunks::default(),
            out: Vec::new(),
            at: 0,
        }))
    }
}

/// A chat-completions reply read as Messages SSE, a line at a time as it
/// arrives: each complete `data:` line is translated by [`Chunks`], and
/// every other line (blank lines, comments, an HTTP error body) passes
/// through, so an error body is reported as on the Messages path. A
/// `data:` line that is not JSON passes through too, and fails the fold
/// the way it would there.
struct Translated {
    inner: BufReader<Reply>,
    chunks: Chunks,
    /// translated bytes not yet read, from `at`
    out: Vec<u8>,
    at: usize,
}

impl Read for Translated {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.at == self.out.len() {
            let mut line = Vec::new();
            if self.inner.read_until(b'\n', &mut line)? == 0 {
                return Ok(0);
            }
            self.out = self.chunks.line(line);
            self.at = 0;
        }
        let n = buf.len().min(self.out.len() - self.at);
        buf[..n].copy_from_slice(&self.out[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// What has been said so far of the reply being translated.
#[derive(Debug, Default)]
struct Chunks {
    /// `message_start` went out
    started: bool,
    /// the index of the text block open, if one is
    open: Option<u64>,
    /// blocks opened so far, the next one's index
    blocks: u64,
    /// the stop reason given, repeated with the usage, since each
    /// `message_delta` sets it
    stop_reason: Value,
}

impl Chunks {
    /// One line of the reply, its newline included, as Messages SSE.
    fn line(&mut self, line: Vec<u8>) -> Vec<u8> {
        let data = line
            .strip_prefix(b"data: ")
            .and_then(|d| d.strip_suffix(b"\n"))
            .map(|d| d.strip_suffix(b"\r").unwrap_or(d));
        let events = match data {
            Some(b"[DONE]") => self.done(),
            Some(d) => match serde_json::from_slice::<Value>(d) {
                Ok(chunk @ Value::Object(_)) => self.chunk(&chunk),
                _ => return line,
            },
            // not a whole `data:` line: as it came
            None => return line,
        };
        let mut out = Vec::new();
        for e in events {
            out.extend_from_slice(b"data: ");
            out.extend_from_slice(e.to_string().as_bytes());
            out.extend_from_slice(b"\n\n");
        }
        out
    }

    /// The Messages events one `chat.completion.chunk` amounts to: the
    /// first opens the message with its model, text deltas go to a text
    /// block opened on the first non-empty one, a `finish_reason` closes
    /// the block and gives the stop reason, the usage chunk the counts. An
    /// `error` in the stream is an `error` event.
    fn chunk(&mut self, chunk: &Value) -> Vec<Value> {
        let mut events = Vec::new();
        match &chunk["error"] {
            Value::Null => {}
            e @ Value::Object(_) => return vec![json!({"type": "error", "error": e})],
            e => return vec![json!({"type": "error", "error": {"message": e}})],
        }
        if !self.started {
            self.started = true;
            events.push(json!({
                "type": "message_start",
                "message": {"model": chunk["model"], "usage": {}},
            }));
        }
        let choice = &chunk["choices"][0];
        if let Some(text) = choice["delta"]["content"]
            .as_str()
            .filter(|t| !t.is_empty())
        {
            let index = match self.open {
                Some(index) => index,
                None => {
                    let index = self.blocks;
                    self.blocks += 1;
                    self.open = Some(index);
                    events.push(json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {"type": "text", "text": ""},
                    }));
                    index
                }
            };
            events.push(json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {"type": "text_delta", "text": text},
            }));
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            events.extend(self.close());
            self.stop_reason = json!(match reason {
                "stop" => "end_turn",
                "length" => "max_tokens",
                other => other,
            });
            events.push(json!({
                "type": "message_delta",
                "delta": {"stop_reason": self.stop_reason},
                "usage": {},
            }));
        }
        if let Some(u) = chunk["usage"].as_object() {
            events.push(json!({
                "type": "message_delta",
                "delta": {"stop_reason": self.stop_reason},
                "usage": usage(u),
            }));
        }
        events
    }

    /// `data: [DONE]`: any open block closed, then `message_stop`.
    fn done(&mut self) -> Vec<Value> {
        let mut events = self.close();
        events.push(json!({"type": "message_stop"}));
        events
    }

    fn close(&mut self) -> Vec<Value> {
        match self.open.take() {
            Some(index) => vec![json!({"type": "content_block_stop", "index": index})],
            None => Vec::new(),
        }
    }
}

/// OpenAI's usage counted as the Messages API counts it: `prompt_tokens`
/// includes the cached ones, which the Messages usage keeps apart as
/// `cache_read_input_tokens`. A count the server did not send is left out,
/// so the usage line prints null, as the script prints a missing field.
fn usage(u: &Map<String, Value>) -> Value {
    let mut m = Map::new();
    let cached = u
        .get("prompt_tokens_details")
        .and_then(|d| d["cached_tokens"].as_u64());
    if let Some(prompt) = u.get("prompt_tokens") {
        let input = match (prompt.as_u64(), cached) {
            (Some(p), Some(c)) => json!(p.saturating_sub(c)),
            _ => prompt.clone(),
        };
        m.insert("input_tokens".into(), input);
    }
    if let Some(c) = cached {
        m.insert("cache_read_input_tokens".into(), json!(c));
    }
    if let Some(out) = u.get("completion_tokens") {
        m.insert("output_tokens".into(), out.clone());
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translate(stream: &str) -> Vec<Value> {
        let mut c = Chunks::default();
        let mut out = Vec::new();
        for line in stream.split_inclusive('\n') {
            out.extend(c.line(line.as_bytes().to_vec()));
        }
        String::from_utf8(out)
            .unwrap()
            .lines()
            .filter_map(|l| l.strip_prefix("data: "))
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// The hand-written fixture folds to the message the history keeps.
    #[test]
    fn the_text_fixture_folds_to_one_text_block() {
        let stream = include_str!("../../tests/fixtures/openai/text.sse");
        let msg = crate::reassemble::message(&translate(stream)).unwrap();
        assert_eq!(
            msg,
            json!({
                "model": "gpt-4.1-mini",
                "stop_reason": "end_turn",
                "stop_details": null,
                "usage": {"input_tokens": 14, "cache_read_input_tokens": 0, "output_tokens": 2},
                "container": null,
                "error": null,
                "content": [{"type": "text", "text": "pong"}],
            })
        );
    }

    /// Cached prompt tokens are the cache read, not input.
    #[test]
    fn cached_prompt_tokens_are_the_cache_read() {
        let u = json!({"prompt_tokens": 100, "completion_tokens": 3,
            "prompt_tokens_details": {"cached_tokens": 64}});
        assert_eq!(
            usage(u.as_object().unwrap()),
            json!({"input_tokens": 36, "cache_read_input_tokens": 64, "output_tokens": 3})
        );
    }

    /// An error object in the stream is the Messages `error` event, and a
    /// bare string error gets the `message` the caller reports.
    #[test]
    fn an_error_chunk_is_an_error_event() {
        let events = translate("data: {\"error\":{\"message\":\"overloaded\"}}\n");
        assert_eq!(
            events,
            [json!({"type": "error", "error": {"message": "overloaded"}})]
        );
        let events = translate("data: {\"error\":\"bad key\"}\n");
        assert_eq!(
            events,
            [json!({"type": "error", "error": {"message": "bad key"}})]
        );
    }

    /// A body that is not SSE passes through byte for byte, and so does a
    /// last `data:` line without its newline.
    #[test]
    fn other_lines_pass_through() {
        let mut c = Chunks::default();
        let body = b"{\"error\":{\"message\":\"no\"}}\n".to_vec();
        assert_eq!(c.line(body.clone()), body);
        assert_eq!(c.line(b"data: {\"a\":1}".to_vec()), b"data: {\"a\":1}");
    }

    /// The history's blocks go back as their text; the system prompt
    /// leads; nothing the shape lacks is sent.
    #[test]
    fn the_body_takes_the_chat_completions_shape() {
        let args = crate::request::Args {
            model: "gpt-4.1-mini".into(),
            max_tokens: 64,
            system: "be brief".into(),
            effort: "medium".into(),
            messages: json!([
                {"role": "user", "content": "first"},
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "hm", "signature": "s"},
                    {"type": "text", "text": "po"},
                    {"type": "text", "text": "ng"},
                ]},
                {"role": "user", "content": "second"},
            ]),
            ..Default::default()
        };
        assert_eq!(
            body(&crate::request::body(&args)).to_string(),
            json!({
                "model": "gpt-4.1-mini",
                "stream": true,
                "max_completion_tokens": 64,
                "stream_options": {"include_usage": true},
                "messages": [
                    {"role": "system", "content": "be brief"},
                    {"role": "user", "content": "first"},
                    {"role": "assistant", "content": "pong"},
                    {"role": "user", "content": "second"},
                ],
            })
            .to_string()
        );
    }
}
